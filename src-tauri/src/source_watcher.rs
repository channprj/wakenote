//! Detection of system-audio sources (Google Meet, YouTube, custom sources …)
//! from the list of shareable windows and owning applications.
//!
//! The on-screen window enumeration (via ScreenCaptureKit's `SCShareableContent`)
//! and the polling timer live in a thin platform wrapper. The decision logic —
//! "given what's on screen now and what was active before, what changed?" — is
//! this pure, testable function. One source is active at a time (it does not
//! switch away from an active source until that source's window disappears,
//! which avoids flapping between e.g. a Meet tab and a YouTube tab).

use crate::sources::{SourceDefinition, match_source};

/// Number of consecutive watcher polls that may miss the active source before
/// the source is considered ended. The watcher currently polls every 5 seconds,
/// so this gives long-running calls roughly 30 seconds of tolerance for
/// transient ScreenCaptureKit enumeration failures or browser-title flapping.
pub const SOURCE_MISSING_GRACE_POLLS: u8 = 6;

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

/// Prefer the first auto-capture-enabled source, then fall back to the first
/// recognized window so disabled sources remain available for manual capture.
fn first_candidate(
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
) -> Option<DetectedSource> {
    let mut fallback = None;
    for window in windows {
        let Some(source) = match_window_source(window, sources) else {
            continue;
        };
        let detected = detected_source(window, source);
        if source.default_auto_prompt {
            return Some(detected);
        }
        fallback.get_or_insert(detected);
    }
    fallback
}

fn first_auto_capture_candidate(
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
) -> Option<DetectedSource> {
    windows.iter().find_map(|window| {
        let source = match_window_source(window, sources)?;
        source
            .default_auto_prompt
            .then(|| detected_source(window, source))
    })
}

fn detected_source(window: &WindowSnapshot, source: &SourceDefinition) -> DetectedSource {
    DetectedSource {
        source_id: source.id.clone(),
        label: source.label.clone(),
        app_name: window.app_name.clone(),
        pid: window.pid,
    }
}

fn auto_capture_replacement(
    active: &DetectedSource,
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
) -> Option<DetectedSource> {
    let active_auto_capture = sources
        .iter()
        .find(|source| source.id == active.source_id)
        .map(|source| source.default_auto_prompt)
        .unwrap_or(false);
    if active_auto_capture {
        return None;
    }
    first_auto_capture_candidate(windows, sources)
        .filter(|candidate| candidate.source_id != active.source_id)
}

/// Is the currently-active source still represented by one of the windows?
fn source_still_present(
    active: &DetectedSource,
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
) -> bool {
    windows.iter().any(|window| {
        window.pid == active.pid
            && match_window_source(window, sources).map(|source| source.id.as_str())
                == Some(active.source_id.as_str())
    })
}

fn match_window_source<'a>(
    window: &WindowSnapshot,
    sources: &'a [SourceDefinition],
) -> Option<&'a SourceDefinition> {
    match_source(&window.title, sources).or_else(|| match_source(&window.app_name, sources))
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
                if let Some(candidate) = auto_capture_replacement(active, windows, sources) {
                    return SourceTransition::Detected(candidate);
                }
                SourceTransition::Unchanged
            } else {
                SourceTransition::Ended(active.clone())
            }
        }
    }
}

/// Compute a transition while tolerating a bounded run of missing active-source
/// polls. The caller keeps `missing_polls` across watcher iterations and stores
/// the returned counter for the next poll.
pub fn compute_source_transition_with_missing_grace(
    previous: Option<&DetectedSource>,
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
    missing_polls: u8,
    required_missing_polls: u8,
) -> (SourceTransition, u8) {
    compute_source_transition_with_missing_grace_and_auto_preemption(
        previous,
        windows,
        sources,
        missing_polls,
        required_missing_polls,
        false,
    )
}

pub fn compute_source_transition_with_missing_grace_and_auto_preemption(
    previous: Option<&DetectedSource>,
    windows: &[WindowSnapshot],
    sources: &[SourceDefinition],
    missing_polls: u8,
    required_missing_polls: u8,
    allow_auto_preemption: bool,
) -> (SourceTransition, u8) {
    match previous {
        None => match first_candidate(windows, sources) {
            Some(detected) => (SourceTransition::Detected(detected), 0),
            None => (SourceTransition::Unchanged, 0),
        },
        Some(active) => {
            if source_still_present(active, windows, sources) {
                if allow_auto_preemption
                    && let Some(candidate) = auto_capture_replacement(active, windows, sources)
                {
                    return (SourceTransition::Detected(candidate), 0);
                }
                return (SourceTransition::Unchanged, 0);
            }

            let required_missing_polls = required_missing_polls.max(1);
            let missing_polls = missing_polls.saturating_add(1);
            if missing_polls >= required_missing_polls {
                (SourceTransition::Ended(active.clone()), 0)
            } else {
                (SourceTransition::Unchanged, missing_polls)
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

pub fn should_defer_source_end_for_recent_audio(
    transition: &SourceTransition,
    already_capturing: bool,
    recent_audio: bool,
) -> bool {
    matches!(transition, SourceTransition::Ended(_)) && already_capturing && recent_audio
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
    fn detects_meet_floating_window_beside_unrelated_chrome_tab() {
        let windows = [
            window("Weekly plan - Confluence", "Google Chrome", 42),
            window("meet.google.com", "Google Chrome", 42),
        ];

        match compute_source_transition(None, &windows, &sources()) {
            SourceTransition::Detected(detected) => {
                assert_eq!(detected.source_id, "meet");
                assert_eq!(detected.app_name, "Google Chrome");
                assert_eq!(detected.pid, 42);
            }
            other => panic!("expected Detected, got {other:?}"),
        }
    }

    #[test]
    fn detects_untitled_google_meet_app_window_by_owning_application() {
        let windows = [window("", "Google Meet", 84)];

        assert!(matches!(
            compute_source_transition(None, &windows, &sources()),
            SourceTransition::Detected(DetectedSource {
                source_id,
                app_name,
                pid: 84,
                ..
            }) if source_id == "meet" && app_name == "Google Meet"
        ));
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
    fn grace_keeps_active_source_through_transient_missing_polls() {
        let active = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        let windows_without_meet = [window("Inbox - Gmail", "Google Chrome", 42)];

        let (transition, missing_polls) = compute_source_transition_with_missing_grace(
            Some(&active),
            &windows_without_meet,
            &sources(),
            0,
            SOURCE_MISSING_GRACE_POLLS,
        );

        assert_eq!(transition, SourceTransition::Unchanged);
        assert_eq!(missing_polls, 1);

        let windows_with_meet = [window("Weekly sync - Google Meet", "Google Chrome", 42)];
        let (transition, missing_polls) = compute_source_transition_with_missing_grace(
            Some(&active),
            &windows_with_meet,
            &sources(),
            missing_polls,
            SOURCE_MISSING_GRACE_POLLS,
        );

        assert_eq!(transition, SourceTransition::Unchanged);
        assert_eq!(missing_polls, 0);
    }

    #[test]
    fn grace_ends_active_source_after_consecutive_missing_polls() {
        let active = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        let windows_without_meet = [window("Inbox - Gmail", "Google Chrome", 42)];
        let mut missing_polls = 0;

        for _ in 1..SOURCE_MISSING_GRACE_POLLS {
            let (transition, next_missing_polls) = compute_source_transition_with_missing_grace(
                Some(&active),
                &windows_without_meet,
                &sources(),
                missing_polls,
                SOURCE_MISSING_GRACE_POLLS,
            );
            assert_eq!(transition, SourceTransition::Unchanged);
            missing_polls = next_missing_polls;
        }

        let (transition, missing_polls) = compute_source_transition_with_missing_grace(
            Some(&active),
            &windows_without_meet,
            &sources(),
            missing_polls,
            SOURCE_MISSING_GRACE_POLLS,
        );

        assert_eq!(transition, SourceTransition::Ended(active));
        assert_eq!(missing_polls, 0);
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
    fn idle_prefers_auto_capture_enabled_source_over_earlier_disabled_source() {
        let mut sources = sources();
        for source in &mut sources {
            source.default_auto_prompt = source.id == "meet";
        }
        let windows = [
            window("Lo-fi - YouTube", "Google Chrome", 10),
            window("Standup - Google Meet", "Google Chrome", 10),
        ];

        match compute_source_transition(None, &windows, &sources) {
            SourceTransition::Detected(detected) => assert_eq!(detected.source_id, "meet"),
            other => panic!("expected Detected(meet), got {other:?}"),
        }
    }

    #[test]
    fn idle_falls_back_to_first_source_when_none_auto_capture() {
        let mut sources = sources();
        for source in &mut sources {
            source.default_auto_prompt = false;
        }
        let windows = [
            window("Lo-fi - YouTube", "Google Chrome", 10),
            window("Standup - Google Meet", "Google Chrome", 10),
        ];

        match compute_source_transition(None, &windows, &sources) {
            SourceTransition::Detected(detected) => assert_eq!(detected.source_id, "youtube"),
            other => panic!("expected Detected(youtube), got {other:?}"),
        }
    }

    #[test]
    fn active_auto_disabled_source_yields_to_auto_capture_candidate() {
        let active = DetectedSource {
            source_id: "youtube".into(),
            label: "YouTube".into(),
            app_name: "Google Chrome".into(),
            pid: 10,
        };
        let mut sources = sources();
        for source in &mut sources {
            source.default_auto_prompt = source.id == "meet";
        }
        let windows = [
            window("Lo-fi - YouTube", "Google Chrome", 10),
            window("Standup - Google Meet", "Google Chrome", 10),
        ];

        match compute_source_transition_with_missing_grace_and_auto_preemption(
            Some(&active),
            &windows,
            &sources,
            0,
            SOURCE_MISSING_GRACE_POLLS,
            true,
        )
        .0
        {
            SourceTransition::Detected(detected) => assert_eq!(detected.source_id, "meet"),
            other => panic!("expected Detected(meet), got {other:?}"),
        }
    }

    #[test]
    fn active_manual_capture_stays_sticky_when_an_auto_candidate_appears() {
        let active = DetectedSource {
            source_id: "youtube".into(),
            label: "YouTube".into(),
            app_name: "Google Chrome".into(),
            pid: 10,
        };
        let mut sources = sources();
        for source in &mut sources {
            source.default_auto_prompt = source.id == "meet";
        }
        let windows = [
            window("Lo-fi - YouTube", "Google Chrome", 10),
            window("Standup - Google Meet", "Google Chrome", 10),
        ];

        assert_eq!(
            compute_source_transition_with_missing_grace_and_auto_preemption(
                Some(&active),
                &windows,
                &sources,
                0,
                SOURCE_MISSING_GRACE_POLLS,
                false,
            )
            .0,
            SourceTransition::Unchanged
        );
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
    fn recent_audio_defers_auto_stop_for_missing_active_source() {
        let active = DetectedSource {
            source_id: "meet".into(),
            label: "Google Meet".into(),
            app_name: "Google Chrome".into(),
            pid: 42,
        };
        let ended = SourceTransition::Ended(active);

        assert!(should_defer_source_end_for_recent_audio(&ended, true, true));
        assert!(!should_defer_source_end_for_recent_audio(
            &ended, true, false
        ));
        assert!(!should_defer_source_end_for_recent_audio(
            &ended, false, true
        ));
        assert!(!should_defer_source_end_for_recent_audio(
            &SourceTransition::Unchanged,
            true,
            true
        ));
    }

    #[test]
    fn detects_custom_source_when_definition_is_provided() {
        let sources = vec![crate::sources::SourceDefinition {
            id: "zoom".into(),
            label: "Zoom".into(),
            description: "Zoom desktop app or windows titled Zoom Meeting".into(),
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

    #[test]
    fn detects_custom_source_by_app_name_when_title_does_not_match() {
        let sources = vec![crate::sources::SourceDefinition {
            id: "spotify".into(),
            label: "Spotify".into(),
            description: "Spotify".into(),
            title_patterns: vec!["spotify".into()],
            default_auto_prompt: true,
            custom: true,
        }];
        let windows = [window("Song Title - Artist", "Spotify", 5819)];

        match compute_source_transition(None, &windows, &sources) {
            SourceTransition::Detected(detected) => {
                assert_eq!(detected.source_id, "spotify");
                assert_eq!(detected.label, "Spotify");
                assert_eq!(detected.pid, 5819);
            }
            other => panic!("expected Detected(spotify), got {other:?}"),
        }
    }
}
