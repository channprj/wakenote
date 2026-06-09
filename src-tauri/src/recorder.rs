use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::settings::{AppSettings, AudioFormat};
use crate::storage::{OutputTarget, next_available_output};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    NotRequested,
    Queued,
    Completed,
    Failed,
}

/// Which audio source a chunk came from. `Microphone` is the default so chunk
/// metadata written before this field existed deserializes unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChunkSource {
    #[default]
    Microphone,
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub model_id: String,
    pub device_id: String,
    pub device_name: String,
    pub sample_rate: u32,
    pub threshold_dbfs: f32,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub duration_ms: i64,
    pub transcription_status: TranscriptionStatus,
    pub app_version: String,
    pub used_fallback_device: bool,
    #[serde(default)]
    pub live_capture_chunk_id: Option<u64>,
    #[serde(default)]
    pub source: ChunkSource,
    #[serde(default)]
    pub source_label: Option<String>,
}

#[derive(Debug)]
pub struct RecordingRequest<'a> {
    pub save_root: &'a Path,
    pub settings: &'a AppSettings,
    pub samples: &'a [f32],
    pub sample_rate: u32,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub device_id: &'a str,
    pub device_name: &'a str,
    pub used_fallback_device: bool,
    pub transcription_enabled: bool,
    pub app_version: &'a str,
    pub live_capture_chunk_id: Option<u64>,
    pub source: ChunkSource,
    pub source_label: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedChunk {
    pub audio_path: std::path::PathBuf,
    pub metadata_path: std::path::PathBuf,
    pub transcript_path: std::path::PathBuf,
    pub error_path: std::path::PathBuf,
}

impl RecordedChunk {
    pub fn from_audio_path(audio_path: impl Into<std::path::PathBuf>) -> Self {
        let audio_path = audio_path.into();
        Self {
            metadata_path: audio_path.with_extension("json"),
            transcript_path: audio_path.with_extension("txt"),
            error_path: audio_path.with_extension("error.txt"),
            audio_path,
        }
    }
}

#[derive(Debug, Error)]
pub enum RecorderError {
    #[error("m4a encoding requires the native macOS encoder bridge")]
    M4aRequiresNativeBridge,
    #[error("native m4a encoder failed: {0}")]
    M4aEncoder(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("wav error: {0}")]
    Wav(#[from] hound::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct Recorder;

impl Recorder {
    pub fn write_chunk(request: RecordingRequest<'_>) -> Result<RecordedChunk, RecorderError> {
        let target = next_available_output(
            request.save_root,
            request.started_at,
            request.settings.audio_format,
            request.source_label,
        )?;

        match request.settings.audio_format {
            AudioFormat::Wav => {
                write_wav(&target.audio_path, request.samples, request.sample_rate)?;
            }
            AudioFormat::M4a => {
                write_m4a(&target.audio_path, request.samples, request.sample_rate)?;
            }
        }

        let metadata = ChunkMetadata {
            model_id: request.settings.selected_model.clone(),
            device_id: request.device_id.to_string(),
            device_name: request.device_name.to_string(),
            sample_rate: request.sample_rate,
            threshold_dbfs: request.settings.threshold_dbfs,
            started_at: request.started_at,
            ended_at: request.ended_at,
            duration_ms: (request.ended_at - request.started_at).num_milliseconds(),
            transcription_status: if request.transcription_enabled {
                TranscriptionStatus::Queued
            } else {
                TranscriptionStatus::NotRequested
            },
            app_version: request.app_version.to_string(),
            used_fallback_device: request.used_fallback_device,
            live_capture_chunk_id: request.live_capture_chunk_id,
            source: request.source,
            source_label: request.source_label.map(str::to_string),
        };
        write_metadata(&target.metadata_path, &metadata)?;

        Ok(recorded_chunk(target))
    }
}

pub struct TranscriptionSidecar;

impl TranscriptionSidecar {
    pub fn write_success(chunk: &RecordedChunk, transcript: &str) -> Result<(), RecorderError> {
        write_text_sidecar(&chunk.transcript_path, transcript)?;
        if chunk.error_path.exists() {
            fs::remove_file(&chunk.error_path)?;
        }
        update_metadata_status_if_present(&chunk.metadata_path, TranscriptionStatus::Completed)
    }

    pub fn write_error(chunk: &RecordedChunk, error: &str) -> Result<(), RecorderError> {
        write_text_sidecar(&chunk.error_path, error)?;
        if chunk.transcript_path.exists() {
            fs::remove_file(&chunk.transcript_path)?;
        }
        update_metadata_status_if_present(&chunk.metadata_path, TranscriptionStatus::Failed)
    }
}

fn recorded_chunk(target: OutputTarget) -> RecordedChunk {
    RecordedChunk {
        audio_path: target.audio_path,
        metadata_path: target.metadata_path,
        transcript_path: target.transcript_path,
        error_path: target.error_path,
    }
}

fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), RecorderError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        writer.write_sample((clamped * i16::MAX as f32) as i16)?;
    }
    writer.finalize()?;
    Ok(())
}

fn write_m4a(path: &Path, samples: &[f32], sample_rate: u32) -> Result<(), RecorderError> {
    let temp_wav_path = path.with_extension("encoding.wav");
    write_wav(&temp_wav_path, samples, sample_rate)?;
    let output = Command::new("/usr/bin/afconvert")
        .arg("-f")
        .arg("m4af")
        .arg("-d")
        .arg("aac")
        .arg(&temp_wav_path)
        .arg(path)
        .output()?;

    let _ = fs::remove_file(&temp_wav_path);
    if !output.status.success() {
        let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(RecorderError::M4aEncoder(if message.is_empty() {
            format!("afconvert exited with status {}", output.status)
        } else {
            message
        }));
    }

    Ok(())
}

fn write_metadata(path: &Path, metadata: &ChunkMetadata) -> Result<(), RecorderError> {
    let bytes = serde_json::to_vec_pretty(metadata)?;
    fs::write(path, bytes)?;
    Ok(())
}

fn write_text_sidecar(path: &Path, text: &str) -> Result<(), RecorderError> {
    let mut file = fs::File::create(path)?;
    writeln!(file, "{text}")?;
    Ok(())
}

fn update_metadata_status(path: &Path, status: TranscriptionStatus) -> Result<(), RecorderError> {
    let mut metadata: ChunkMetadata = serde_json::from_slice(&fs::read(path)?)?;
    metadata.transcription_status = status;
    write_metadata(path, &metadata)
}

fn update_metadata_status_if_present(
    path: &Path,
    status: TranscriptionStatus,
) -> Result<(), RecorderError> {
    if path.exists() {
        update_metadata_status(path, status)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_metadata(source: ChunkSource, source_label: Option<String>) -> ChunkMetadata {
        let now = Utc::now();
        ChunkMetadata {
            model_id: "whisper-medium".into(),
            device_id: "default".into(),
            device_name: "System Default".into(),
            sample_rate: 16_000,
            threshold_dbfs: -42.0,
            started_at: now,
            ended_at: now,
            duration_ms: 0,
            transcription_status: TranscriptionStatus::Queued,
            app_version: "0.0.0".into(),
            used_fallback_device: false,
            live_capture_chunk_id: Some(1),
            source,
            source_label,
        }
    }

    #[test]
    fn chunk_source_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&ChunkSource::System).unwrap(),
            "\"system\""
        );
        assert_eq!(ChunkSource::default(), ChunkSource::Microphone);
    }

    #[test]
    fn metadata_round_trips_system_source() {
        let meta = sample_metadata(ChunkSource::System, Some("meet".into()));
        let json = serde_json::to_string(&meta).unwrap();
        let back: ChunkMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(back.source, ChunkSource::System);
        assert_eq!(back.source_label.as_deref(), Some("meet"));
    }

    #[test]
    fn legacy_metadata_without_source_defaults_to_microphone() {
        // Sidecar JSON written before source/source_label existed.
        let json = r#"{
            "model_id": "whisper-medium", "device_id": "default",
            "device_name": "System Default", "sample_rate": 16000,
            "threshold_dbfs": -42.0,
            "started_at": "2026-06-09T01:02:03Z", "ended_at": "2026-06-09T01:02:04Z",
            "duration_ms": 1000, "transcription_status": "completed",
            "app_version": "0.1.0", "used_fallback_device": false
        }"#;
        let meta: ChunkMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.source, ChunkSource::Microphone);
        assert_eq!(meta.source_label, None);
        assert_eq!(meta.live_capture_chunk_id, None);
    }
}
