use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::commands::{
    RecentTranscript, transcript_days_from_save_root, transcripts_for_day_result_from_save_root,
};
use crate::recorder::ChunkSource;
use crate::settings::CustomSourceEntry;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptExportResult {
    pub destination_path: String,
    pub day_count: usize,
    pub transcript_count: usize,
}

pub fn export_all_transcript_days(
    save_root: &Path,
    destination_parent: &Path,
    custom_sources: &[CustomSourceEntry],
) -> Result<TranscriptExportResult, String> {
    export_all_transcript_days_at(save_root, destination_parent, custom_sources, Local::now())
}

pub fn export_transcript_day(
    save_root: &Path,
    day: &str,
    destination_path: &Path,
    custom_sources: &[CustomSourceEntry],
) -> Result<TranscriptExportResult, String> {
    if destination_path.exists() {
        return Err(format!(
            "export destination already exists: {}",
            destination_path.display()
        ));
    }
    let parent = destination_path
        .parent()
        .ok_or_else(|| "export destination has no parent folder".to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "export destination folder does not exist: {}",
            parent.display()
        ));
    }

    let entries = transcripts_for_day_result_from_save_root(save_root, day, true)?;
    if entries.is_empty() {
        return Err(format!("No transcripts to export for {day}"));
    }

    let partial_path = next_available_partial_file(destination_path)?;
    if let Err(error) = fs::write(
        &partial_path,
        format_transcript_day(&entries, custom_sources),
    ) {
        let _ = fs::remove_file(&partial_path);
        return Err(format!(
            "failed to write transcript export {}: {error}",
            destination_path.display()
        ));
    }
    if let Err(error) = fs::hard_link(&partial_path, destination_path) {
        let _ = fs::remove_file(&partial_path);
        return Err(format!(
            "failed to publish transcript export {}: {error}",
            destination_path.display()
        ));
    }
    let _ = fs::remove_file(&partial_path);

    Ok(TranscriptExportResult {
        destination_path: destination_path.to_string_lossy().to_string(),
        day_count: 1,
        transcript_count: entries.len(),
    })
}

fn export_all_transcript_days_at(
    save_root: &Path,
    destination_parent: &Path,
    custom_sources: &[CustomSourceEntry],
    now: DateTime<Local>,
) -> Result<TranscriptExportResult, String> {
    let destination_metadata = fs::metadata(destination_parent).map_err(|error| {
        format!(
            "failed to open export destination {}: {error}",
            destination_parent.display()
        )
    })?;
    if !destination_metadata.is_dir() {
        return Err(format!(
            "export destination is not a folder: {}",
            destination_parent.display()
        ));
    }

    let days = transcript_days_from_save_root(save_root);
    if days.is_empty() {
        return Err("No transcripts to export".into());
    }

    let base_name = format!("WakeNote Transcripts {}", now.format("%Y-%m-%d %H%M%S"));
    let final_path = next_available_archive_path(destination_parent, &base_name);
    let partial_path = next_available_partial_path(destination_parent, &base_name);
    fs::create_dir(&partial_path).map_err(|error| {
        format!(
            "failed to prepare export destination {}: {error}",
            destination_parent.display()
        )
    })?;

    let write_result = (|| {
        let mut day_count = 0;
        let mut transcript_count = 0;
        for day in days {
            let entries = transcripts_for_day_result_from_save_root(save_root, &day.day, true)?;
            if entries.is_empty() {
                continue;
            }
            let destination = partial_path.join(format!("WakeNote-{}.txt", day.day));
            fs::write(
                &destination,
                format_transcript_day(&entries, custom_sources),
            )
            .map_err(|error| {
                format!(
                    "failed to write transcript export {}: {error}",
                    destination.display()
                )
            })?;
            day_count += 1;
            transcript_count += entries.len();
        }
        if day_count == 0 {
            return Err("No transcripts to export".into());
        }
        Ok((day_count, transcript_count))
    })();

    let (day_count, transcript_count) = match write_result {
        Ok(counts) => counts,
        Err(error) => {
            let _ = fs::remove_dir_all(&partial_path);
            return Err(error);
        }
    };

    if let Err(error) = fs::rename(&partial_path, &final_path) {
        let _ = fs::remove_dir_all(&partial_path);
        return Err(format!(
            "failed to publish transcript export {}: {error}",
            final_path.display()
        ));
    }

    Ok(TranscriptExportResult {
        destination_path: final_path.to_string_lossy().to_string(),
        day_count,
        transcript_count,
    })
}

fn next_available_archive_path(parent: &Path, base_name: &str) -> PathBuf {
    for suffix in 1.. {
        let name = if suffix == 1 {
            base_name.to_string()
        } else {
            format!("{base_name} ({suffix})")
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("archive suffix loop is unbounded")
}

fn next_available_partial_path(parent: &Path, base_name: &str) -> PathBuf {
    for attempt in 1.. {
        let candidate = parent.join(format!(
            ".{base_name}.partial-{}-{attempt}",
            std::process::id()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("partial suffix loop is unbounded")
}

fn next_available_partial_file(destination: &Path) -> Result<PathBuf, String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "export destination has no parent folder".to_string())?;
    let file_name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "export destination file name is not valid UTF-8".to_string())?;
    for attempt in 1.. {
        let candidate = parent.join(format!(
            ".{file_name}.partial-{}-{attempt}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    unreachable!("partial file suffix loop is unbounded")
}

fn format_transcript_day(
    entries: &[RecentTranscript],
    custom_sources: &[CustomSourceEntry],
) -> String {
    let custom_labels = custom_sources
        .iter()
        .map(|source| (source.id.as_str(), source.label.as_str()))
        .collect::<HashMap<_, _>>();
    let mut output = entries
        .iter()
        .map(|entry| {
            format!(
                "{} [{}] - {}",
                local_timestamp(&entry.recorded_at),
                transcript_source_label(entry, &custom_labels),
                entry.text.trim(),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    output
}

fn local_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

fn transcript_source_label(
    entry: &RecentTranscript,
    custom_labels: &HashMap<&str, &str>,
) -> String {
    if entry.source != ChunkSource::System {
        return match entry.source_label.as_deref() {
            Some("dictation") => "Dict".into(),
            Some("mic-merged") => "Merged Mic".into(),
            Some("mic-priority") => "Priority Mic".into(),
            _ => entry
                .device_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or("Mic")
                .to_string(),
        };
    }

    let Some(source_label) = entry
        .source_label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
    else {
        return "System".into();
    };
    if let Some(custom_label) = custom_labels.get(source_label) {
        return (*custom_label).to_string();
    }
    match source_label {
        "meet" => "Meet".into(),
        "youtube" => "YouTube".into(),
        value => human_source_label(value),
    }
}

fn human_source_label(value: &str) -> String {
    value
        .split(['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().chain(characters).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::commands::RecentTranscript;
    use crate::recorder::ChunkSource;
    use crate::settings::CustomSourceEntry;
    use chrono::{Local, TimeZone};

    use super::{export_all_transcript_days_at, export_transcript_day, format_transcript_day};

    fn system_entry(recorded_at: &str, source_label: &str, text: &str) -> RecentTranscript {
        RecentTranscript {
            transcript_path: "/tmp/transcript.txt".into(),
            audio_path: None,
            recorded_at: recorded_at.into(),
            text: text.into(),
            source: ChunkSource::System,
            source_label: Some(source_label.into()),
            device_id: None,
            device_name: None,
            microphone_slot: None,
        }
    }

    fn microphone_entry(
        source_label: Option<&str>,
        device_name: Option<&str>,
        text: &str,
    ) -> RecentTranscript {
        RecentTranscript {
            transcript_path: "/tmp/transcript.txt".into(),
            audio_path: None,
            recorded_at: "2026-08-24T01:02:03+09:00".into(),
            text: text.into(),
            source: ChunkSource::Microphone,
            source_label: source_label.map(str::to_string),
            device_id: None,
            device_name: device_name.map(str::to_string),
            microphone_slot: None,
        }
    }

    #[test]
    fn formats_entries_with_local_timestamps_custom_labels_and_single_newlines() {
        let entries = vec![
            system_entry("2026-08-24T01:02:03+09:00", "meet", "첫 번째\n\n"),
            system_entry("2026-08-24T02:03:04+09:00", "youtube", "\r\n두 번째\r\n"),
        ];
        let custom_sources = vec![CustomSourceEntry {
            id: "meet".into(),
            label: "Team call".into(),
            title_patterns: vec![],
            auto_prompt: false,
        }];

        assert_eq!(
            format_transcript_day(&entries, &custom_sources),
            concat!(
                "2026-08-24 01:02:03 [Team call] - 첫 번째\n",
                "2026-08-24 02:03:04 [YouTube] - 두 번째\n",
            ),
        );
    }

    #[test]
    fn formats_microphone_source_labels_like_the_transcript_list() {
        let entries = vec![
            microphone_entry(Some("dictation"), Some("Built-in Mic"), "dictation"),
            microphone_entry(Some("mic-merged"), Some("Merged Device"), "merged"),
            microphone_entry(Some("mic-priority"), Some("Priority Device"), "priority"),
            microphone_entry(None, Some("  Studio Mic  "), "device"),
            microphone_entry(None, Some("  "), "fallback"),
        ];

        let exported = format_transcript_day(&entries, &[]);

        assert!(exported.contains("[Dict] - dictation"), "{exported}");
        assert!(exported.contains("[Merged Mic] - merged"), "{exported}");
        assert!(exported.contains("[Priority Mic] - priority"), "{exported}");
        assert!(exported.contains("[Studio Mic] - device"), "{exported}");
        assert!(exported.contains("[Mic] - fallback"), "{exported}");
    }

    #[test]
    fn formats_blank_system_source_as_system() {
        let exported = format_transcript_day(
            &[system_entry(
                "2026-08-24T01:02:03+09:00",
                "",
                "system audio",
            )],
            &[],
        );

        assert!(exported.contains("[System] - system audio"), "{exported}");
    }

    #[test]
    fn exports_all_days_to_collision_safe_daily_files() {
        let source = tempfile::tempdir().expect("source");
        let destination = tempfile::tempdir().expect("destination");
        let first_day = source.path().join("20260823");
        let second_day = source.path().join("20260824");
        let uploaded_day = source.path().join("uploaded/20260823");
        fs::create_dir_all(&first_day).expect("first day");
        fs::create_dir_all(&second_day).expect("second day");
        fs::create_dir_all(&uploaded_day).expect("uploaded day");
        fs::write(first_day.join("090000.txt"), "morning").expect("morning");
        fs::write(uploaded_day.join("180000.txt"), "uploaded").expect("uploaded");
        fs::write(second_day.join("120000.txt"), "today").expect("today");
        let now = Local
            .with_ymd_and_hms(2026, 8, 24, 15, 35, 0)
            .single()
            .expect("local time");

        let first = export_all_transcript_days_at(source.path(), destination.path(), &[], now)
            .expect("first export");
        let second = export_all_transcript_days_at(source.path(), destination.path(), &[], now)
            .expect("second export");

        assert_eq!(first.day_count, 2);
        assert_eq!(first.transcript_count, 3);
        assert!(
            first
                .destination_path
                .ends_with("WakeNote Transcripts 2026-08-24 153500")
        );
        assert!(
            second
                .destination_path
                .ends_with("WakeNote Transcripts 2026-08-24 153500 (2)")
        );
        assert_eq!(
            fs::read_to_string(
                std::path::Path::new(&first.destination_path).join("WakeNote-2026-08-23.txt"),
            )
            .expect("first file")
            .lines()
            .map(|line| line.rsplit(" - ").next().unwrap_or_default())
            .collect::<Vec<_>>(),
            vec!["morning", "uploaded"],
        );
        assert!(
            fs::read_to_string(
                std::path::Path::new(&first.destination_path).join("WakeNote-2026-08-24.txt"),
            )
            .expect("second file")
            .ends_with(" - today\n"),
        );
    }

    #[test]
    fn failed_archive_export_removes_partial_output() {
        let source = tempfile::tempdir().expect("source");
        let destination = tempfile::tempdir().expect("destination");
        let day = source.path().join("20260824");
        fs::create_dir_all(&day).expect("day");
        fs::write(day.join("090000.txt"), [0xff, 0xfe]).expect("invalid UTF-8");
        let now = Local
            .with_ymd_and_hms(2026, 8, 24, 15, 35, 0)
            .single()
            .expect("local time");

        let error = export_all_transcript_days_at(source.path(), destination.path(), &[], now)
            .expect_err("export must fail");

        assert!(error.contains("090000.txt"), "{error}");
        assert_eq!(
            fs::read_dir(destination.path())
                .expect("destination listing")
                .count(),
            0,
        );
    }

    #[test]
    fn exports_one_day_without_overwriting_an_existing_destination() {
        let source = tempfile::tempdir().expect("source");
        let destination = tempfile::tempdir().expect("destination");
        let day = source.path().join("20260824");
        fs::create_dir_all(&day).expect("day");
        fs::write(day.join("090000.txt"), "morning").expect("morning");
        fs::write(day.join("180000.txt"), "evening").expect("evening");
        let destination_path = destination.path().join("WakeNote-2026-08-24.txt");

        let result = export_transcript_day(source.path(), "2026-08-24", &destination_path, &[])
            .expect("day export");

        assert_eq!(result.day_count, 1);
        assert_eq!(result.transcript_count, 2);
        assert_eq!(result.destination_path, destination_path.to_string_lossy());
        let original = fs::read_to_string(&destination_path).expect("exported day");
        assert!(original.contains(" - morning\n"), "{original}");
        assert!(!original.contains("\n\n"), "{original}");
        assert!(original.ends_with(" - evening\n"), "{original}");

        let error = export_transcript_day(source.path(), "2026-08-24", &destination_path, &[])
            .expect_err("existing destination must not be overwritten");

        assert!(error.contains("already exists"), "{error}");
        assert_eq!(
            fs::read_to_string(&destination_path).expect("unchanged export"),
            original,
        );
    }
}
