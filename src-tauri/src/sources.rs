//! Recognized system-audio capture sources (Google Meet, YouTube, …).
//!
//! The list is fixed and code-defined. The only per-source user setting is
//! whether detection auto-prompts — see [`crate::settings::resolve_auto_prompt`].

/// A system-audio source WakeNote can recognize by window title.
pub struct RecognizedSource {
    /// Stable id. Doubles as the metadata `source_label` and the audio file slug.
    pub id: &'static str,
    /// Human-facing label shown in settings / notifications.
    pub label: &'static str,
    /// Lower-cased substrings; a window whose (lower-cased) title contains any of
    /// these is attributed to this source.
    pub title_patterns: &'static [&'static str],
    /// Whether detection auto-prompts for capture by default.
    pub default_auto_prompt: bool,
}

const RECOGNIZED_SOURCES: &[RecognizedSource] = &[
    RecognizedSource {
        id: "meet",
        label: "Google Meet",
        title_patterns: &["google meet", "meet - "],
        default_auto_prompt: true,
    },
    RecognizedSource {
        id: "youtube",
        label: "YouTube",
        title_patterns: &["- youtube", "youtube"],
        default_auto_prompt: false,
    },
];

pub fn recognized_sources() -> &'static [RecognizedSource] {
    RECOGNIZED_SOURCES
}

pub fn recognized_source(id: &str) -> Option<&'static RecognizedSource> {
    RECOGNIZED_SOURCES.iter().find(|source| source.id == id)
}

/// Attribute a window title to a recognized source, if any. Case-insensitive.
/// Sources are checked in list order, so earlier entries win when a title would
/// match more than one (e.g. a Meet tab whose title also mentions YouTube).
pub fn match_recognized_source(window_title: &str) -> Option<&'static RecognizedSource> {
    let title = window_title.to_lowercase();
    if title.trim().is_empty() {
        return None;
    }
    RECOGNIZED_SOURCES.iter().find(|source| {
        source
            .title_patterns
            .iter()
            .any(|pattern| title.contains(pattern))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_google_meet_titles() {
        assert_eq!(
            match_recognized_source("Meet - 주간회의").map(|s| s.id),
            Some("meet")
        );
        assert_eq!(
            match_recognized_source("팀 회의 - Google Meet").map(|s| s.id),
            Some("meet")
        );
    }

    #[test]
    fn matches_youtube_titles_case_insensitively() {
        assert_eq!(
            match_recognized_source("Lo-fi beats - YouTube - Chrome").map(|s| s.id),
            Some("youtube")
        );
        assert_eq!(
            match_recognized_source("YouTube Music").map(|s| s.id),
            Some("youtube")
        );
    }

    #[test]
    fn ignores_unrelated_or_empty_titles() {
        assert!(match_recognized_source("Slack | general").is_none());
        // 한글 "유튜브"는 ASCII "youtube" 패턴과 매칭되지 않는다(오탐 방지).
        assert!(match_recognized_source("내 유튜브 정리 메모").is_none());
        assert!(match_recognized_source("").is_none());
        assert!(match_recognized_source("   ").is_none());
    }

    #[test]
    fn meet_wins_over_youtube_on_overlap() {
        assert_eq!(
            match_recognized_source("Google Meet — shared YouTube tab").map(|s| s.id),
            Some("meet")
        );
    }

    #[test]
    fn lookup_by_id_and_list_shape() {
        assert!(recognized_source("meet").is_some());
        assert!(recognized_source("youtube").is_some());
        assert!(recognized_source("zoom").is_none());
        assert_eq!(recognized_sources().len(), 2);
    }

    #[test]
    fn default_auto_prompt_meet_on_youtube_off() {
        assert!(recognized_source("meet").unwrap().default_auto_prompt);
        assert!(!recognized_source("youtube").unwrap().default_auto_prompt);
    }
}
