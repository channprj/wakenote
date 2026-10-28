//! System-audio capture sources (Google Meet, YouTube, user-defined, …).

use crate::settings::AppSettings;

/// A system-audio source WakeNote can recognize by shareable-window title or app name.
pub struct RecognizedSource {
    /// Stable id. Doubles as the metadata `source_label` and the audio file slug.
    pub id: &'static str,
    /// Human-facing label shown in settings / notifications.
    pub label: &'static str,
    /// Human-facing explanation of which windows/apps this source recognizes.
    pub description: &'static str,
    /// Lower-cased substrings; a window whose (lower-cased) title or owning app
    /// name contains any of these is attributed to this source.
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
    pub description: String,
    pub title_patterns: Vec<String>,
    pub default_auto_prompt: bool,
    pub custom: bool,
}

const RECOGNIZED_SOURCES: &[RecognizedSource] = &[
    RecognizedSource {
        id: "meet",
        label: "Google Meet",
        description: "Google Meet tabs and floating meet.google.com call windows",
        title_patterns: &["google meet", "meet - ", "meet.google.com"],
        default_auto_prompt: true,
    },
    RecognizedSource {
        id: "zoom",
        label: "Zoom",
        description: "Zoom desktop app or windows titled Zoom Meeting",
        title_patterns: &["zoom", "zoom meeting"],
        default_auto_prompt: true,
    },
    RecognizedSource {
        id: "youtube",
        label: "YouTube",
        description: "YouTube tabs, videos, and YouTube Music windows",
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
            description: source.description.to_string(),
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
                description: source.title_patterns.join(", "),
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
    let title = normalize_source_text(window_title);
    if title.trim().is_empty() {
        return None;
    }
    sources.iter().find(|source| {
        source
            .title_patterns
            .iter()
            .map(|pattern| normalize_source_text(pattern))
            .any(|pattern| title.contains(&pattern))
    })
}

/// Attribute a window title to a recognized source, if any. Case-insensitive.
/// Sources are checked in list order, so earlier entries win when a title would
/// match more than one (e.g. a Meet tab whose title also mentions YouTube).
pub fn match_recognized_source(window_title: &str) -> Option<&'static RecognizedSource> {
    let title = normalize_source_text(window_title);
    if title.trim().is_empty() {
        return None;
    }
    RECOGNIZED_SOURCES.iter().find(|source| {
        source
            .title_patterns
            .iter()
            .any(|pattern| title.contains(&normalize_source_text(pattern)))
    })
}

fn normalize_source_text(value: &str) -> String {
    let mut normalized_punctuation = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '-' | '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}'
        ) {
            normalized_punctuation.push_str(" - ");
        } else {
            normalized_punctuation.extend(character.to_lowercase());
        }
    }
    normalized_punctuation
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_recognized_source_attributes_titles() {
        for (title, expected) in [
            // Google Meet tabs, including en/em dash separators.
            ("Meet - 주간회의", Some("meet")),
            ("팀 회의 - Google Meet", Some("meet")),
            ("Meet – 주간 회의", Some("meet")),
            ("Meet—Product review", Some("meet")),
            // Google Meet floating call window.
            ("meet.google.com", Some("meet")),
            ("example.com", None),
            ("Meeting notes", None),
            // YouTube, case-insensitively.
            ("Lo-fi beats - YouTube - Chrome", Some("youtube")),
            ("YouTube Music", Some("youtube")),
            // Zoom, case-insensitively.
            ("Daily sync - Zoom Meeting", Some("zoom")),
            ("Zoom Workplace", Some("zoom")),
            // Unrelated or empty titles.
            ("Slack | general", None),
            // 한글 "유튜브"는 ASCII "youtube" 패턴과 매칭되지 않는다(오탐 방지).
            ("내 유튜브 정리 메모", None),
            ("", None),
            ("   ", None),
            // Meet wins over YouTube when a title mentions both.
            ("Google Meet — shared YouTube tab", Some("meet")),
        ] {
            assert_eq!(
                match_recognized_source(title).map(|source| source.id),
                expected,
                "{title:?}"
            );
        }
    }

    #[test]
    fn builtin_sources_are_listed_and_auto_prompt_by_default() {
        for id in ["meet", "youtube", "zoom"] {
            assert!(recognized_source(id).expect(id).default_auto_prompt, "{id}");
        }
        assert_eq!(recognized_sources().len(), 3);
    }

    #[test]
    fn matches_custom_source_titles_from_settings() {
        let mut settings = crate::settings::AppSettings::default();
        settings.custom_sources = vec![crate::settings::CustomSourceEntry {
            id: "spotify".into(),
            label: "Spotify".into(),
            title_patterns: vec!["Spotify".into()],
            auto_prompt: true,
        }];
        let sources = source_definitions(&settings);

        assert_eq!(
            match_source("Lo-fi mix - Spotify", &sources).map(|source| source.id.as_str()),
            Some("spotify")
        );
    }
}
