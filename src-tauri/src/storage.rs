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
) -> std::io::Result<OutputTarget> {
    // File paths use local wall-clock for human readability; metadata JSON keeps UTC for archival.
    let local = timestamp.with_timezone(&Local);
    let date_dir = local.format("%Y%m%d").to_string();
    let stem = local.format("%H%M%S").to_string();
    let extension = match format {
        AudioFormat::M4a => "m4a",
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
