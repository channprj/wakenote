use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::settings::DictionaryEntry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAX_DICTIONARY_FILE_BYTES: usize = 1024 * 1024;
pub const MAX_DICTIONARY_ACTIVE_LINES: usize = 1_000;
const DICTIONARY_HEADER: &str = "# Canonical term = aliases";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryFileError {
    message: String,
    line: Option<usize>,
}

impl DictionaryFileError {
    fn at_line(line: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: Some(line),
        }
    }

    fn file(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
        }
    }

    pub fn line(&self) -> Option<usize> {
        self.line
    }
}

impl fmt::Display for DictionaryFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(line) = self.line {
            write!(formatter, "line {line}: {}", self.message)
        } else {
            formatter.write_str(&self.message)
        }
    }
}

impl std::error::Error for DictionaryFileError {}

fn comparison_key(value: &str) -> String {
    if value.is_ascii() {
        value.to_ascii_lowercase()
    } else {
        value.to_string()
    }
}

fn allocate_dictionary_id(used_ids: &mut HashSet<String>, next_id: &mut usize) -> String {
    loop {
        let candidate = format!("dictionary-{}", *next_id);
        *next_id += 1;
        if used_ids.insert(candidate.clone()) {
            return candidate;
        }
    }
}

pub fn parse_dictionary_text(
    text: &str,
    previous: &[DictionaryEntry],
) -> Result<Vec<DictionaryEntry>, DictionaryFileError> {
    if text.len() > MAX_DICTIONARY_FILE_BYTES {
        return Err(DictionaryFileError::file(
            "dictionary.txt exceeds the 1 MiB limit",
        ));
    }

    let previous_by_term = previous
        .iter()
        .map(|entry| (comparison_key(entry.term.trim()), entry))
        .collect::<HashMap<_, _>>();
    let mut used_ids = HashSet::new();
    let mut seen_terms = HashSet::new();
    let mut parsed_rows = Vec::new();

    for (zero_based_line, raw_line) in text.lines().enumerate() {
        let line_number = zero_based_line + 1;
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if parsed_rows.len() >= MAX_DICTIONARY_ACTIVE_LINES {
            return Err(DictionaryFileError::at_line(
                line_number,
                format!(
                    "dictionary.txt supports at most {MAX_DICTIONARY_ACTIVE_LINES} active lines"
                ),
            ));
        }

        let (raw_term, raw_aliases) = line
            .split_once('=')
            .map(|(term, aliases)| (term, Some(aliases)))
            .unwrap_or((line, None));
        let term = raw_term.trim();
        if term.is_empty() {
            return Err(DictionaryFileError::at_line(
                line_number,
                "canonical term is required",
            ));
        }
        let term_key = comparison_key(term);
        if !seen_terms.insert(term_key.clone()) {
            return Err(DictionaryFileError::at_line(
                line_number,
                format!("duplicate canonical term: {term}"),
            ));
        }

        let mut seen_aliases = HashSet::new();
        let aliases = match raw_aliases {
            None => Vec::new(),
            Some(raw_aliases) => {
                let mut aliases = Vec::new();
                for raw_alias in raw_aliases.split(',') {
                    let alias = raw_alias.trim();
                    if alias.is_empty() {
                        return Err(DictionaryFileError::at_line(
                            line_number,
                            "alias cannot be empty",
                        ));
                    }
                    let alias_key = comparison_key(alias);
                    if alias_key != term_key && seen_aliases.insert(alias_key) {
                        aliases.push(alias.to_string());
                    }
                }
                aliases
            }
        };

        parsed_rows.push((term.to_string(), term_key, aliases));
    }

    for (_, term_key, _) in &parsed_rows {
        if let Some(previous_entry) = previous_by_term.get(term_key) {
            used_ids.insert(previous_entry.id.clone());
        }
    }
    let mut next_id = 1;
    Ok(parsed_rows
        .into_iter()
        .map(|(term, term_key, aliases)| {
            let (id, enabled) = previous_by_term
                .get(&term_key)
                .map(|entry| (entry.id.clone(), entry.enabled))
                .unwrap_or_else(|| (allocate_dictionary_id(&mut used_ids, &mut next_id), true));
            DictionaryEntry {
                id,
                term,
                aliases,
                enabled,
            }
        })
        .collect())
}

pub fn format_dictionary_text(entries: &[DictionaryEntry]) -> String {
    let mut lines = Vec::with_capacity(entries.len() + 1);
    lines.push(DICTIONARY_HEADER.to_string());
    lines.extend(entries.iter().filter_map(|entry| {
        let term = entry.term.trim();
        if term.is_empty() {
            return None;
        }
        let aliases = entry
            .aliases
            .iter()
            .map(|alias| alias.trim())
            .filter(|alias| !alias.is_empty())
            .collect::<Vec<_>>();
        Some(if aliases.is_empty() {
            term.to_string()
        } else {
            format!("{term} = {}", aliases.join(", "))
        })
    }));
    format!("{}\n", lines.join("\n"))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DictionaryFileStatus {
    pub path: String,
    pub revision: String,
    pub error: Option<String>,
    pub error_line: Option<usize>,
    pub in_sync: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictionaryPoll {
    Unchanged,
    Changed(Vec<DictionaryEntry>),
    Invalid(DictionaryFileError),
}

pub struct DictionaryFileStore {
    path: PathBuf,
    last_good_hash: String,
    last_seen_hash: String,
    status: DictionaryFileStatus,
}

impl DictionaryFileStore {
    pub fn new(path: PathBuf) -> Self {
        let path_text = path.to_string_lossy().into_owned();
        Self {
            path,
            last_good_hash: String::new(),
            last_seen_hash: String::new(),
            status: DictionaryFileStatus {
                path: path_text,
                revision: String::new(),
                error: None,
                error_line: None,
                in_sync: false,
            },
        }
    }

    pub fn ensure(
        &mut self,
        current: &[DictionaryEntry],
    ) -> Result<DictionaryPoll, DictionaryFileError> {
        if self.path.exists() {
            return Ok(self.reload(current));
        }
        self.save_entries(current)?;
        Ok(DictionaryPoll::Unchanged)
    }

    pub fn save_entries(&mut self, entries: &[DictionaryEntry]) -> Result<(), DictionaryFileError> {
        let text = format_dictionary_text(entries);
        let bytes = text.as_bytes();
        match atomic_write(&self.path, bytes) {
            Ok(()) => {
                let hash = sha256_hex(bytes);
                self.mark_good(hash);
                Ok(())
            }
            Err(error) => {
                let error = DictionaryFileError::file(format!(
                    "could not write {}: {error}",
                    self.path.display()
                ));
                self.mark_invalid(&error);
                Err(error)
            }
        }
    }

    pub fn poll(&mut self, current: &[DictionaryEntry]) -> DictionaryPoll {
        self.read_and_parse(current, false)
    }

    pub fn reload(&mut self, current: &[DictionaryEntry]) -> DictionaryPoll {
        self.read_and_parse(current, true)
    }

    pub fn status(&self) -> DictionaryFileStatus {
        self.status.clone()
    }

    pub fn editor_command(&self) -> Command {
        let mut command = Command::new("/usr/bin/open");
        command.arg(&self.path);
        command
    }

    pub fn open_in_default_editor(&self) -> Result<(), DictionaryFileError> {
        self.editor_command().spawn().map(|_| ()).map_err(|error| {
            DictionaryFileError::file(format!("could not open {}: {error}", self.path.display()))
        })
    }

    fn read_and_parse(&mut self, current: &[DictionaryEntry], force: bool) -> DictionaryPoll {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) => {
                let error = DictionaryFileError::file(format!(
                    "could not read {}: {error}",
                    self.path.display()
                ));
                self.mark_invalid(&error);
                return DictionaryPoll::Invalid(error);
            }
        };
        let hash = sha256_hex(&bytes);
        if !force && hash == self.last_seen_hash {
            return DictionaryPoll::Unchanged;
        }
        self.last_seen_hash = hash.clone();

        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                let error = DictionaryFileError::file("dictionary.txt must be valid UTF-8");
                self.mark_invalid(&error);
                return DictionaryPoll::Invalid(error);
            }
        };
        match parse_dictionary_text(text, current) {
            Ok(entries) => {
                self.mark_good(hash);
                if entries == current {
                    DictionaryPoll::Unchanged
                } else {
                    DictionaryPoll::Changed(entries)
                }
            }
            Err(error) => {
                self.mark_invalid(&error);
                DictionaryPoll::Invalid(error)
            }
        }
    }

    fn mark_good(&mut self, hash: String) {
        self.last_good_hash = hash.clone();
        self.last_seen_hash = hash.clone();
        self.status.revision = hash;
        self.status.error = None;
        self.status.error_line = None;
        self.status.in_sync = true;
    }

    fn mark_invalid(&mut self, error: &DictionaryFileError) {
        self.status.revision = self.last_good_hash.clone();
        self.status.error = Some(error.to_string());
        self.status.error_line = error.line();
        self.status.in_sync = false;
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("dictionary.txt");
    let temporary =
        path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));

    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::DictionaryEntry;
    use std::fs;

    fn entry(id: &str, term: &str, aliases: &[&str], enabled: bool) -> DictionaryEntry {
        DictionaryEntry {
            id: id.to_string(),
            term: term.to_string(),
            aliases: aliases.iter().map(|alias| (*alias).to_string()).collect(),
            enabled,
        }
    }

    #[test]
    fn parses_comments_aliases_and_standalone_terms_in_order() {
        let previous = [entry("saved-wakenote", "WakeNote", &["old alias"], false)];
        let parsed = parse_dictionary_text(
            "# Canonical term = aliases\nWakeNote = wake note, wake-note\n\nStandalone\nQwen3 ASR = qwen 3 asr, 큐원 ASR\n",
            &previous,
        )
        .expect("valid dictionary");

        assert_eq!(
            parsed,
            vec![
                entry(
                    "saved-wakenote",
                    "WakeNote",
                    &["wake note", "wake-note"],
                    false,
                ),
                entry("dictionary-1", "Standalone", &[], true),
                entry(
                    "dictionary-2",
                    "Qwen3 ASR",
                    &["qwen 3 asr", "큐원 ASR"],
                    true,
                ),
            ]
        );
    }

    #[test]
    fn rejects_duplicate_canonical_term_with_exact_line() {
        let error = parse_dictionary_text("WakeNote\nwakenote\n", &[]).unwrap_err();
        assert_eq!(error.line(), Some(2));
        assert!(error.to_string().contains("line 2"));
    }

    #[test]
    fn rejects_missing_canonical_term_with_exact_line() {
        let error = parse_dictionary_text("WakeNote\n = wake note\n", &[]).unwrap_err();
        assert_eq!(error.line(), Some(2));
        assert!(error.to_string().contains("line 2"));
    }

    #[test]
    fn rejects_empty_alias_token_with_exact_line() {
        let error = parse_dictionary_text("WakeNote = wake note, , wake-note\n", &[]).unwrap_err();
        assert_eq!(error.line(), Some(1));
        assert!(error.to_string().contains("line 1"));
    }

    #[test]
    fn rejects_more_than_one_thousand_active_lines() {
        let text = (1..=1_001)
            .map(|index| format!("Term {index}"))
            .collect::<Vec<_>>()
            .join("\n");
        let error = parse_dictionary_text(&text, &[]).unwrap_err();
        assert_eq!(error.line(), Some(1_001));
    }

    #[test]
    fn rejects_input_larger_than_one_mebibyte() {
        let text = "a".repeat(MAX_DICTIONARY_FILE_BYTES + 1);
        let error = parse_dictionary_text(&text, &[]).unwrap_err();
        assert_eq!(error.line(), None);
        assert!(error.to_string().contains("1 MiB"));
    }

    #[test]
    fn formatter_emits_stable_header_and_round_trips() {
        let entries = vec![
            entry(
                "dictionary-1",
                "WakeNote",
                &["wake note", "wake-note"],
                true,
            ),
            entry("dictionary-2", "Standalone", &[], true),
        ];
        let text = format_dictionary_text(&entries);

        assert_eq!(
            text,
            "# Canonical term = aliases\nWakeNote = wake note, wake-note\nStandalone\n"
        );
        assert_eq!(parse_dictionary_text(&text, &entries).unwrap(), entries);
    }

    #[test]
    fn ensure_creates_absent_file_from_current_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let entries = vec![entry(
            "dictionary-1",
            "WakeNote",
            &["wake note", "wake-note"],
            true,
        )];
        let mut store = DictionaryFileStore::new(path.clone());

        assert_eq!(store.ensure(&entries).unwrap(), DictionaryPoll::Unchanged);
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            format_dictionary_text(&entries)
        );
        assert!(store.status().in_sync);
    }

    #[test]
    fn poll_returns_changed_entries_for_valid_external_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let current = vec![entry("dictionary-7", "WakeNote", &["old"], false)];
        let mut store = DictionaryFileStore::new(path.clone());
        store.ensure(&current).unwrap();
        fs::write(&path, "WakeNote = wake note\nQwen3 ASR = qwen 3 asr\n").unwrap();

        assert_eq!(
            store.poll(&current),
            DictionaryPoll::Changed(vec![
                entry("dictionary-7", "WakeNote", &["wake note"], false),
                entry("dictionary-1", "Qwen3 ASR", &["qwen 3 asr"], true),
            ])
        );
        assert!(store.status().in_sync);
    }

    #[test]
    fn poll_after_self_write_is_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let entries = vec![entry("dictionary-1", "WakeNote", &["wake note"], true)];
        let mut store = DictionaryFileStore::new(path);
        store.save_entries(&entries).unwrap();

        assert_eq!(store.poll(&entries), DictionaryPoll::Unchanged);
    }

    #[test]
    fn invalid_external_contents_preserve_last_good_revision_and_report_line() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let entries = vec![entry("dictionary-1", "WakeNote", &["wake note"], true)];
        let mut store = DictionaryFileStore::new(path.clone());
        store.ensure(&entries).unwrap();
        let last_good_revision = store.status().revision.clone();
        fs::write(&path, "WakeNote\nwakenote\n").unwrap();

        let DictionaryPoll::Invalid(error) = store.poll(&entries) else {
            panic!("expected invalid dictionary poll");
        };
        assert_eq!(error.line(), Some(2));
        assert_eq!(store.status().revision, last_good_revision);
        assert_eq!(store.status().error_line, Some(2));
        assert!(!store.status().in_sync);
        assert_eq!(store.poll(&entries), DictionaryPoll::Unchanged);
    }

    #[test]
    fn atomic_save_leaves_no_temporary_sibling() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("dictionary.txt");
        let entries = vec![entry("dictionary-1", "WakeNote", &[], true)];
        let mut store = DictionaryFileStore::new(path);
        store.save_entries(&entries).unwrap();

        let names = fs::read_dir(directory.path())
            .unwrap()
            .map(|item| item.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["dictionary.txt"]);
    }

    #[test]
    fn editor_command_uses_explicit_open_and_stable_path() {
        let path = std::path::PathBuf::from("/tmp/WakeNote/dictionary.txt");
        let store = DictionaryFileStore::new(path.clone());
        let command = store.editor_command();

        assert_eq!(command.get_program(), "/usr/bin/open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![path.as_os_str()]
        );
    }
}
