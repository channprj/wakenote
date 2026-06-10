//! System-audio capture sources (Google Meet, YouTube, user-defined, …).

use crate::settings::AppSettings;

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

/// Runtime source definition used by the watcher. Built-ins and user-defined
/// sources share this shape so matching stays in one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDefinition {
    pub id: String,
    pub label: String,
    pub title_patterns: Vec<String>,
    pub default_auto_prompt: bool,
    pub custom: bool,
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
        default_auto_prompt: true,
    },
];

pub fn recognized_sources() -> &'static [RecognizedSource] {
    RECOGNIZED_SOURCES
}

pub fn recognized_source(id: &str) -> Option<&'static RecognizedSource> {
    RECOGNIZED_SOURCES.iter().find(|source| source.id == id)
}

pub fn source_definitions(settings: &AppSettings) -> Vec<SourceDefinition> {
    let mut sources = RECOGNIZED_SOURCES
        .iter()
        .map(|source| SourceDefinition {
            id: source.id.to_string(),
            label: source.label.to_string(),
            title_patterns: source
                .title_patterns
                .iter()
                .map(|pattern| pattern.to_string())
                .collect(),
            default_auto_prompt: source.default_auto_prompt,
            custom: false,
        })
        .collect::<Vec<_>>();

    sources.extend(
        settings
            .custom_sources
            .iter()
            .map(|source| SourceDefinition {
                id: source.id.clone(),
                label: source.label.clone(),
                title_patterns: source.title_patterns.clone(),
                default_auto_prompt: source.auto_prompt,
                custom: true,
            }),
    );

    sources
}

pub fn match_source<'a>(
    window_title: &str,
    sources: &'a [SourceDefinition],
) -> Option<&'a SourceDefinition> {
    let title = window_title.to_lowercase();
    if title.trim().is_empty() {
        return None;
    }
    sources.iter().find(|source| {
        source
            .title_patterns
            .iter()
            .map(|pattern| pattern.to_lowercase())
            .any(|pattern| title.contains(&pattern))
    })
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
    fn default_auto_prompt_meet_and_youtube_on() {
        assert!(recognized_source("meet").unwrap().default_auto_prompt);
        assert!(recognized_source("youtube").unwrap().default_auto_prompt);
    }

    #[test]
    fn matches_custom_source_titles_from_settings() {
        let mut settings = crate::settings::AppSettings::default();
        settings.custom_sources = vec![crate::settings::CustomSourceEntry {
            id: "zoom".into(),
            label: "Zoom".into(),
            title_patterns: vec!["Zoom Meeting".into()],
            auto_prompt: true,
        }];
        let sources = source_definitions(&settings);

        assert_eq!(
            match_source("Daily sync - Zoom Meeting", &sources).map(|source| source.id.as_str()),
            Some("zoom")
        );
    }
}
