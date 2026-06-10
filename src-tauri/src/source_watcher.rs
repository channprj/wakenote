//! Detection of system-audio sources (Google Meet, YouTube, custom sources …)
//! from the list of on-screen windows.
//!
//! The on-screen window enumeration (via ScreenCaptureKit's `SCShareableContent`)
//! and the polling timer live in a thin platform wrapper. The decision logic —
//! "given what's on screen now and what was active before, what changed?" — is
//! this pure, testable function. One source is active at a time (it does not
//! switch away from an active source until that source's window disappears,
//! which avoids flapping between e.g. a Meet tab and a YouTube tab).

use crate::sources::{SourceDefinition, match_source};

/// A window observed on screen, reduced to what detection needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSnapshot {
    pub title: String,
    pub app_name: String,
    pub pid: i32,
}

/// A recognized source currently detected on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedSource {
    /// Recognized source id ("meet" | "youtube"); also the metadata/file slug.
    pub source_id: String,
    pub label: String,
    /// Owning application name (the capture target), e.g. "Google Chrome".
    pub app_name: String,
    /// Owning application process id (the ScreenCaptureKit filter target).
    pub pid: i32,
}

/// What changed between the previously-active source and the current windows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceTransition {
    Detected(DetectedSource),
    Ended(DetectedSource),
    Unchanged,
}

/// First window (in enumeration order) that maps to a recognized source.
fn first_candidate(
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
) -> Option<DetectedSource> {
    windows.iter().find_map(|window| {
        match_source(&window.title, sources).map(|source| DetectedSource {
            source_id: source.id.clone(),
            label: source.label.clone(),
            app_name: window.app_name.clone(),
            pid: window.pid,
        })
    })
}

/// Is the currently-active source still represented by one of the windows?
fn source_still_present(
    active: &DetectedSource,
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
) -> bool {
    windows.iter().any(|window| {
        window.pid == active.pid
            && match_source(&window.title, sources).map(|source| source.id.as_str())
                == Some(active.source_id.as_str())
    })
}

/// Compute the per-poll transition. The caller threads state: on `Detected` set
/// the active source; on `Ended` clear it; on `Unchanged` keep it. Switching
/// from one source to another therefore takes two polls (Ended, then Detected),
/// which is intentional — it keeps a live call from being dropped the instant an
/// unrelated recognized tab appears.
pub fn compute_source_transition(
    previous: Option<&DetectedSource>,
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
) -> SourceTransition {
    match previous {
        None => match first_candidate(windows, sources) {
            Some(detected) => SourceTransition::Detected(detected),
            None => SourceTransition::Unchanged,
        },
        Some(active) => {
            if source_still_present(active, windows, sources) {
                SourceTransition::Unchanged
            } else {
                SourceTransition::Ended(active.clone())
            }
        }
    }
}

/// Whether a freshly-detected source should start capturing automatically. The
/// per-source auto-capture preference is resolved by the caller via
/// [`crate::settings::resolve_auto_prompt`]; this only combines the gating flags
/// so the policy is testable in one place.
pub fn should_auto_capture_source(
    system_audio_enabled: bool,
    auto_capture: bool,
    already_capturing: bool,
    paused_this_session: bool,
) -> bool {
    system_audio_enabled && auto_capture && !already_capturing && !paused_this_session
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::source_definitions;

    fn sources() -> Vec<SourceDefinition> {
        source_definitions(&crate::settings::AppSettings::default())
    }

    fn window(title: &str, app: &str, pid: i32) -> WindowSnapshot {
        WindowSnapshot {
            title: title.to_string(),
            app_name: app.to_string(),
            pid,
        }
    }

    #[test]
    fn detects_when_idle_and_a_meet_window_appears() {
        let windows = [window("Weekly sync - Google Meet", "Google Chrome", 42)];
        match compute_source_transition(None, &windows, &sources()) {
            SourceTransition::Detected(d) => {
                assert_eq!(d.source_id, "meet");
                assert_eq!(d.app_name, "Google Chrome");
                assert_eq!(d.pid, 42);
            }
            other => panic!("expected Detected, got {other:?}"),
        }
    }

    #[test]
    fn unchanged_while_active_source_still_present() {
        let active = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        let windows = [window("Weekly sync - Google Meet", "Google Chrome", 42)];
        assert_eq!(
            compute_source_transition(Some(&active), &windows, &sources()),
            SourceTransition::Unchanged
        );
    }

    #[test]
    fn ends_when_active_source_window_disappears() {
        let active = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        let windows = [window("Inbox - Gmail", "Google Chrome", 42)];
        assert_eq!(
            compute_source_transition(Some(&active), &windows, &sources()),
            SourceTransition::Ended(active)
        );
    }

    #[test]
    fn idle_ignores_unrecognized_windows() {
        let windows = [window("Slack | general", "Slack", 7)];
        assert_eq!(
            compute_source_transition(None, &windows, &sources()),
            SourceTransition::Unchanged
        );
    }

    #[test]
    fn first_matching_window_wins() {
        let windows = [
            window("Lo-fi - YouTube", "Google Chrome", 10),
            window("Standup - Google Meet", "Google Chrome", 10),
        ];
        match compute_source_transition(None, &windows, &sources()) {
            SourceTransition::Detected(d) => assert_eq!(d.source_id, "youtube"),
            other => panic!("expected Detected(youtube), got {other:?}"),
        }
    }

    #[test]
    fn switching_sources_takes_two_polls() {
        let active = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        // Meet gone, YouTube present in a different process: first poll ends Meet.
        let windows = [window("Lo-fi - YouTube", "Safari", 99)];
        assert_eq!(
            compute_source_transition(Some(&active), &windows, &sources()),
            SourceTransition::Ended(active)
        );
        // After the caller clears the active source, the next poll detects YouTube.
        match compute_source_transition(None, &windows, &sources()) {
            SourceTransition::Detected(d) => {
                assert_eq!(d.source_id, "youtube");
                assert_eq!(d.pid, 99);
            }
            other => panic!("expected Detected(youtube), got {other:?}"),
        }
    }

    #[test]
    fn auto_captures_only_when_enabled_preferred_idle_and_not_paused() {
        assert!(should_auto_capture_source(true, true, false, false));
        // master off
        assert!(!should_auto_capture_source(false, true, false, false));
        // auto-capture off -> manual resume/start only
        assert!(!should_auto_capture_source(true, false, false, false));
        // already capturing
        assert!(!should_auto_capture_source(true, true, true, false));
        // paused for this session
        assert!(!should_auto_capture_source(true, true, false, true));
    }

    #[test]
    fn detects_custom_source_when_definition_is_provided() {
        let sources = vec![crate::sources::SourceDefinition {
            id: "zoom".into(),
            label: "Zoom".into(),
            title_patterns: vec!["zoom meeting".into()],
            default_auto_prompt: true,
            custom: true,
        }];
        let windows = [window("Daily sync - Zoom Meeting", "zoom.us", 77)];

        match compute_source_transition(None, &windows, &sources) {
            SourceTransition::Detected(detected) => {
                assert_eq!(detected.source_id, "zoom");
                assert_eq!(detected.label, "Zoom");
                assert_eq!(detected.pid, 77);
            }
            other => panic!("expected Detected(zoom), got {other:?}"),
        }
    }
}
