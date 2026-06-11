use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, Utc};

use crate::settings::AudioFormat;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputBasename(String);

impl OutputBasename {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputTarget {
    pub basename: OutputBasename,
    pub audio_path: PathBuf,
    pub metadata_path: PathBuf,
    pub transcript_path: PathBuf,
    pub error_path: PathBuf,
}

pub fn next_available_output(
    save_root: &Path,
    timestamp: DateTime<Utc>,
    format: AudioFormat,
    source_slug: Option<&str>,
) -> std::io::Result<OutputTarget> {
    // File paths use local wall-clock for human readability; metadata JSON keeps UTC for archival.
    let local = timestamp.with_timezone(&Local);
    let date_dir = local.format("%Y%m%d").to_string();
    // Non-mic sources get a `-{slug}` suffix (e.g. HHMMSS-meet) so a system chunk
    // never collides with a mic chunk that started the same second.
    let stem = match source_slug.and_then(normalize_source_slug) {
        Some(slug) => format!("{}-{slug}", local.format("%H%M%S")),
        None => local.format("%H%M%S").to_string(),
    };
    let extension = match format {
        AudioFormat::M4a => "m4a",
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Wav => "wav",
    };
    let directory = save_root.join(date_dir);
    std::fs::create_dir_all(&directory)?;

    for index in 0..1_000 {
        let basename = collision_basename(&stem, index);
        let audio_path = directory.join(format!("{basename}.{extension}"));
        let metadata_path = directory.join(format!("{basename}.json"));
        let transcript_path = directory.join(format!("{basename}.txt"));
        let error_path = directory.join(format!("{basename}.error.txt"));

        if audio_path.exists()
            || metadata_path.exists()
            || transcript_path.exists()
            || error_path.exists()
        {
            continue;
        }

        return Ok(OutputTarget {
            basename: OutputBasename::new(basename),
            audio_path,
            metadata_path,
            transcript_path,
            error_path,
        });
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no available filename after 999 collisions",
    ))
}

fn collision_basename(stem: &str, index: usize) -> String {
    if index == 0 {
        stem.to_string()
    } else {
        // index 1 → "-2", index 2 → "-3" … keep names short and human-friendly.
        format!("{stem}-{}", index + 1)
    }
}

fn normalize_source_slug(source_slug: &str) -> Option<String> {
    let mut normalized = String::new();
    let mut pending_separator = false;
    for character in source_slug.chars() {
        if character.is_ascii_alphanumeric() {
            if pending_separator && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(character.to_ascii_lowercase());
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub fn uploaded_audio_target(
    save_root: &Path,
    source_path: &Path,
    timestamp: DateTime<Local>,
) -> std::io::Result<PathBuf> {
    if !is_uploadable_audio_path(source_path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only mp3, m4a, and wav audio files can be uploaded",
        ));
    }

    let file_name = source_path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .filter(|file_name| !file_name.trim().is_empty())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "audio filename is missing",
            )
        })?;
    let directory = save_root
        .join("uploaded")
        .join(timestamp.format("%Y%m%d").to_string());
    std::fs::create_dir_all(&directory)?;
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name);
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();

    for index in 0..1_000 {
        let basename = collision_basename(stem, index);
        let candidate = directory.join(format!("{basename}.{extension}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no available upload filename after 999 collisions",
    ))
}

pub fn copy_uploaded_audio_file(
    save_root: &Path,
    source_path: &Path,
    timestamp: DateTime<Local>,
) -> std::io::Result<PathBuf> {
    if !source_path.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "upload source is not a file",
        ));
    }

    let target = uploaded_audio_target(save_root, source_path, timestamp)?;
    std::fs::copy(source_path, &target)?;
    Ok(target)
}

fn is_uploadable_audio_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp3" | "m4a" | "wav"
            )
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AudioFormat;
    use chrono::{DateTime, TimeZone, Utc};

    fn ts() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 9, 1, 2, 3).unwrap()
    }

    #[test]
    fn source_slug_suffixes_the_stem() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = next_available_output(tmp.path(), ts(), AudioFormat::M4a, Some("meet"))
            .expect("target");
        let name = target.audio_path.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with("-meet.m4a"), "got {name}");
    }

    #[test]
    fn source_slug_suffix_is_normalized_for_human_labels() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target = next_available_output(tmp.path(), ts(), AudioFormat::M4a, Some("Spotify"))
            .expect("target");
        let name = target.audio_path.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with("-spotify.m4a"), "got {name}");
    }

    #[test]
    fn mic_path_has_no_source_suffix() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let target =
            next_available_output(tmp.path(), ts(), AudioFormat::M4a, None).expect("target");
        let name = target.audio_path.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with(".m4a"));
        assert!(!name.contains('-'), "unexpected suffix in {name}");
    }

    #[test]
    fn source_slug_collision_rolls_over() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let first =
            next_available_output(tmp.path(), ts(), AudioFormat::Wav, Some("meet")).expect("first");
        std::fs::write(&first.audio_path, b"x").expect("write first");
        let second = next_available_output(tmp.path(), ts(), AudioFormat::Wav, Some("meet"))
            .expect("second");
        let name = second.audio_path.file_name().unwrap().to_string_lossy();
        assert!(name.ends_with("-meet-2.wav"), "got {name}");
    }
}
