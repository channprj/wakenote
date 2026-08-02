use std::cmp::Reverse;
use std::collections::HashSet;

use crate::settings::{AppSettings, DictionaryEntry};

const DICTIONARY_PROMPT_MAX_CHARS: usize = 800;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryContext {
    canonical_terms: Vec<String>,
    prompt: Option<String>,
    replacements: Vec<(String, String)>,
    serialized_entries: String,
}

impl Default for DictionaryContext {
    fn default() -> Self {
        Self {
            canonical_terms: Vec::new(),
            prompt: None,
            replacements: Vec::new(),
            serialized_entries: "[]".to_string(),
        }
    }
}

impl DictionaryContext {
    pub fn from_settings(settings: &AppSettings) -> Self {
        Self::compile(settings.dictionary_enabled, &settings.dictionary)
    }

    pub fn compile(enabled: bool, entries: &[DictionaryEntry]) -> Self {
        if !enabled {
            return Self::default();
        }

        let active_entries = entries
            .iter()
            .filter(|entry| entry.enabled)
            .filter_map(|entry| {
                let term = entry.term.trim();
                if term.is_empty() {
                    return None;
                }
                Some(DictionaryEntry {
                    id: entry.id.clone(),
                    term: term.to_string(),
                    aliases: entry
                        .aliases
                        .iter()
                        .map(|alias| alias.trim())
                        .filter(|alias| !alias.is_empty())
                        .map(str::to_string)
                        .collect(),
                    enabled: true,
                })
            })
            .collect::<Vec<_>>();

        let canonical_terms = active_entries
            .iter()
            .map(|entry| entry.term.clone())
            .collect::<Vec<_>>();
        let prompt = compile_prompt(&canonical_terms);

        let mut seen_aliases = HashSet::new();
        let mut replacements = active_entries
            .iter()
            .flat_map(|entry| {
                entry
                    .aliases
                    .iter()
                    .map(move |alias| (alias.clone(), entry.term.clone()))
            })
            .filter(|(alias, canonical)| !dictionary_values_equal(alias, canonical))
            .filter(|(alias, _)| seen_aliases.insert(dictionary_comparison_key(alias)))
            .collect::<Vec<_>>();
        replacements.sort_by_key(|(alias, _)| Reverse(alias.chars().count()));

        Self {
            canonical_terms,
            prompt,
            replacements,
            serialized_entries: serde_json::to_string(&active_entries)
                .unwrap_or_else(|_| "[]".to_string()),
        }
    }

    pub fn canonical_terms(&self) -> &[String] {
        &self.canonical_terms
    }

    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    pub fn serialized_entries(&self) -> &str {
        if self.serialized_entries.is_empty() {
            "[]"
        } else {
            &self.serialized_entries
        }
    }

    pub fn correct(&self, text: &str) -> String {
        if text.is_empty() || self.replacements.is_empty() {
            return text.to_string();
        }

        let mut corrected = String::with_capacity(text.len());
        let mut index = 0;
        while index < text.len() {
            let matched = self
                .replacements
                .iter()
                .find(|(alias, _)| alias_matches_at(text, index, alias));
            if let Some((alias, canonical)) = matched {
                corrected.push_str(canonical);
                index += alias.len();
                continue;
            }

            let character = text[index..]
                .chars()
                .next()
                .expect("index remains on a character boundary");
            corrected.push(character);
            index += character.len_utf8();
        }
        corrected
    }
}

fn compile_prompt(terms: &[String]) -> Option<String> {
    let mut prompt = String::new();
    for term in terms {
        let delimiter_chars = usize::from(!prompt.is_empty()) * 2;
        if prompt.chars().count() + delimiter_chars + term.chars().count()
            > DICTIONARY_PROMPT_MAX_CHARS
        {
            continue;
        }
        if !prompt.is_empty() {
            prompt.push_str(", ");
        }
        prompt.push_str(term);
    }
    (!prompt.is_empty()).then_some(prompt)
}

fn dictionary_comparison_key(value: &str) -> String {
    if value.is_ascii() {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

fn dictionary_values_equal(left: &str, right: &str) -> bool {
    dictionary_comparison_key(left) == dictionary_comparison_key(right)
}

fn alias_matches_at(text: &str, index: usize, alias: &str) -> bool {
    let Some(end) = index.checked_add(alias.len()) else {
        return false;
    };
    let Some(candidate) = text.get(index..end) else {
        return false;
    };

    if !alias.is_ascii() {
        return candidate == alias;
    }
    if !candidate.eq_ignore_ascii_case(alias) {
        return false;
    }

    let left_is_alphanumeric = text[..index]
        .chars()
        .next_back()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    let right_is_alphanumeric = text[end..]
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    !left_is_alphanumeric && !right_is_alphanumeric
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(term: &str, aliases: &[&str]) -> DictionaryEntry {
        DictionaryEntry {
            id: term.to_ascii_lowercase(),
            term: term.to_string(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            enabled: true,
        }
    }

    #[test]
    fn correction_uses_boundaries_and_longest_alias_first() {
        let context = DictionaryContext::compile(
            true,
            &[
                entry("WakeNote", &["wake note", "wake"]),
                entry("Qwen3-ASR", &["qwen 3 asr"]),
            ],
        );

        assert_eq!(
            context.correct("WAKE NOTE uses qwen 3 asr; awake stays."),
            "WakeNote uses Qwen3-ASR; awake stays."
        );
    }

    #[test]
    fn correction_replaces_non_ascii_aliases_exactly() {
        let context = DictionaryContext::compile(true, &[entry("웨이크노트", &["웨이크 노트"])]);

        assert_eq!(context.correct("웨이크 노트입니다"), "웨이크노트입니다");
    }

    #[test]
    fn disabled_dictionary_and_entries_compile_to_no_effect() {
        let entry = entry("WakeNote", &["wake note"]);
        assert_eq!(
            DictionaryContext::compile(false, std::slice::from_ref(&entry)),
            DictionaryContext::default()
        );

        let context = DictionaryContext::compile(
            true,
            &[DictionaryEntry {
                enabled: false,
                ..entry
            }],
        );
        assert!(context.canonical_terms().is_empty());
        assert_eq!(context.correct("wake note"), "wake note");
    }

    #[test]
    fn prompt_contains_only_complete_terms_within_the_budget() {
        let oversized = "x".repeat(DICTIONARY_PROMPT_MAX_CHARS + 1);
        let context = DictionaryContext::compile(
            true,
            &[
                entry("WakeNote", &[]),
                entry(&oversized, &[]),
                entry("Qwen3-ASR", &[]),
            ],
        );

        assert_eq!(context.prompt(), Some("WakeNote, Qwen3-ASR"));
        assert_eq!(context.canonical_terms().len(), 3);
    }

    #[test]
    fn serialized_entries_contain_only_enabled_entries() {
        let context = DictionaryContext::compile(
            true,
            &[
                entry("WakeNote", &["wake note"]),
                DictionaryEntry {
                    enabled: false,
                    ..entry("Hidden", &["hidden alias"])
                },
            ],
        );

        let serialized: Vec<DictionaryEntry> =
            serde_json::from_str(context.serialized_entries()).expect("dictionary json");
        assert_eq!(serialized.len(), 1);
        assert_eq!(serialized[0].term, "WakeNote");
    }
}
