use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

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
    let date_dir = timestamp.format("%Y%m%d").to_string();
    let stem = timestamp.format("%H%M%S").to_string();
    let extension = match format {
        AudioFormat::M4a => "m4a",
        AudioFormat::Wav => "wav",
    };
    let directory = save_root.join(date_dir);

    for index in 0..1_000 {
        let basename = if index == 0 {
            stem.clone()
        } else {
            format!("{stem}-{index:03}")
        };
        let audio_path = directory.join(format!("{basename}.{extension}"));

        if audio_path.exists() {
            continue;
        }

        return Ok(OutputTarget {
            basename: OutputBasename::new(basename.clone()),
            audio_path,
            metadata_path: directory.join(format!("{basename}.json")),
            transcript_path: directory.join(format!("{basename}.txt")),
            error_path: directory.join(format!("{basename}.error.txt")),
        });
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "no available filename after 999 collisions",
    ))
}
