use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;

use crate::live_capture::{AudioInputBackend, AudioInputConfig, LiveCaptureRuntime};
use crate::settings::{AppSettings, SettingsPatch, TranscriptionLanguage};
use crate::transcription::{
    Transcriber, TranscriptionRequest, resample_linear, should_skip_low_signal_audio,
};

pub const DICTATION_PRESS_DEBOUNCE: Duration = Duration::from_millis(300);
pub const DICTATION_MAX_RECORDING_DURATION: Duration = Duration::from_secs(10 * 60);
pub const DICTATION_STATE_EVENT: &str = "dictation-state";
static NEXT_TEMP_WAV_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DictationStage {
    Idle,
    Recording,
    Transcribing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DictationStatePayload {
    pub state: DictationStage,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationAction {
    StartRecording,
    StopAndTranscribe,
    Ignore,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutRegistrationChange {
    Unchanged,
    Register(String),
    Unregister(String),
    Replace { previous: String, next: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictationRecording {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

pub struct DictationRuntime<B: AudioInputBackend> {
    capture: LiveCaptureRuntime<B>,
    stage: DictationStage,
    last_press: Option<Instant>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: Option<u32>,
    next_recording_id: u64,
    active_recording_id: Option<u64>,
}

impl<B: AudioInputBackend> DictationRuntime<B> {
    pub fn new(backend: B) -> Self {
        Self {
            capture: LiveCaptureRuntime::new(backend),
            stage: DictationStage::Idle,
            last_press: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: None,
            next_recording_id: 1,
            active_recording_id: None,
        }
    }

    pub fn handle_press_at(&mut self, now: Instant) -> DictationAction {
        if self
            .last_press
            .is_some_and(|last| now.saturating_duration_since(last) < DICTATION_PRESS_DEBOUNCE)
        {
            self.last_press = Some(now);
            return DictationAction::Ignore;
        }
        self.last_press = Some(now);

        match self.stage {
            DictationStage::Idle => {
                self.stage = DictationStage::Recording;
                DictationAction::StartRecording
            }
            DictationStage::Recording => {
                self.stage = DictationStage::Transcribing;
                DictationAction::StopAndTranscribe
            }
            DictationStage::Transcribing => DictationAction::Ignore,
        }
    }

    pub fn stage(&self) -> DictationStage {
        self.stage
    }

    pub fn payload(&self, error: Option<String>) -> DictationStatePayload {
        DictationStatePayload {
            state: self.stage,
            error,
        }
    }

    pub fn start_recording(&mut self, config: AudioInputConfig) -> Result<u64, String> {
        if self.stage != DictationStage::Recording {
            return Err("dictation is not ready to record".to_string());
        }
        let sample_rate = config
            .sample_rate
            .filter(|sample_rate| *sample_rate > 0)
            .ok_or_else(|| "dictation requires a valid sample rate".to_string())?;
        self.samples
            .lock()
            .map_err(|error| error.to_string())?
            .clear();
        let callback_samples = self.samples.clone();
        self.capture
            .start(config, move |frame| {
                if let Ok(mut samples) = callback_samples.lock() {
                    samples.extend_from_slice(&frame.samples);
                }
            })
            .map_err(|error| error.to_string())?;

        let recording_id = self.next_recording_id;
        self.next_recording_id = self.next_recording_id.saturating_add(1);
        self.active_recording_id = Some(recording_id);
        self.sample_rate = Some(sample_rate);
        Ok(recording_id)
    }

    pub fn stop_recording(&mut self) -> Result<DictationRecording, String> {
        if self.stage != DictationStage::Transcribing {
            return Err("dictation is not ready to transcribe".to_string());
        }
        self.capture.stop();
        self.active_recording_id = None;
        let sample_rate = self
            .sample_rate
            .take()
            .ok_or_else(|| "dictation sample rate is unavailable".to_string())?;
        let samples = std::mem::take(&mut *self.samples.lock().map_err(|error| error.to_string())?);
        Ok(DictationRecording {
            samples,
            sample_rate,
        })
    }

    pub fn stop_if_recording(
        &mut self,
        recording_id: u64,
    ) -> Option<Result<DictationRecording, String>> {
        if self.stage != DictationStage::Recording || self.active_recording_id != Some(recording_id)
        {
            return None;
        }
        self.stage = DictationStage::Transcribing;
        Some(self.stop_recording())
    }

    pub fn finish(&mut self) {
        self.capture.stop();
        self.stage = DictationStage::Idle;
        self.sample_rate = None;
        self.active_recording_id = None;
        if let Ok(mut samples) = self.samples.lock() {
            samples.clear();
        }
    }
}

pub fn shortcut_registration_change(
    previous_enabled: bool,
    previous_shortcut: &str,
    next_enabled: bool,
    next_shortcut: &str,
) -> ShortcutRegistrationChange {
    match (previous_enabled, next_enabled) {
        (false, false) => ShortcutRegistrationChange::Unchanged,
        (false, true) => ShortcutRegistrationChange::Register(next_shortcut.to_string()),
        (true, false) => ShortcutRegistrationChange::Unregister(previous_shortcut.to_string()),
        (true, true) if previous_shortcut == next_shortcut => ShortcutRegistrationChange::Unchanged,
        (true, true) => ShortcutRegistrationChange::Replace {
            previous: previous_shortcut.to_string(),
            next: next_shortcut.to_string(),
        },
    }
}

pub fn candidate_dictation_settings(
    previous: &AppSettings,
    patch: &SettingsPatch,
) -> Result<AppSettings, String> {
    let mut patch = patch.clone();
    normalize_dictation_patch(&mut patch)?;
    let mut candidate = previous.clone();
    candidate.apply_patch(patch);
    if candidate.dictation_enabled {
        validate_dictation_shortcut(&candidate.dictation_shortcut)?;
    }
    Ok(candidate)
}

pub fn normalize_dictation_patch(patch: &mut SettingsPatch) -> Result<(), String> {
    if let Some(shortcut) = patch.dictation_shortcut.as_deref() {
        patch.dictation_shortcut = Some(validate_dictation_shortcut(shortcut)?);
    }
    Ok(())
}

pub fn validate_dictation_shortcut(raw: &str) -> Result<String, String> {
    let normalized = raw
        .split('+')
        .map(|part| part.trim().to_lowercase())
        .collect::<Vec<_>>();
    if normalized.is_empty() || normalized.iter().any(String::is_empty) {
        return Err("dictation shortcut cannot be empty".to_string());
    }

    let modifiers = [
        "ctrl", "control", "alt", "option", "shift", "cmd", "command", "meta", "super",
    ];
    if normalized
        .iter()
        .any(|part| matches!(part.as_str(), "fn" | "function"))
    {
        return Err("the fn key is not supported for dictation shortcuts".to_string());
    }
    let main_keys = normalized
        .iter()
        .filter(|part| !modifiers.contains(&part.as_str()))
        .collect::<Vec<_>>();
    if main_keys.len() != 1 {
        return Err("dictation shortcut must contain exactly one main key".to_string());
    }
    let main_key = main_keys[0].as_str();
    let function_key = main_key
        .strip_prefix('f')
        .and_then(|value| value.parse::<u8>().ok())
        .is_some_and(|value| (1..=24).contains(&value));
    let has_modifier = normalized
        .iter()
        .any(|part| modifiers.contains(&part.as_str()));
    if !has_modifier && !function_key {
        return Err(
            "dictation shortcut must include a modifier unless it uses an F-key".to_string(),
        );
    }

    let normalized = normalized.join("+");
    normalized
        .parse::<Shortcut>()
        .map_err(|error| format!("invalid dictation shortcut: {error}"))?;
    Ok(normalized)
}

pub fn prepare_dictation_audio(
    samples: &[f32],
    source_rate: u32,
) -> Result<Option<Vec<f32>>, String> {
    if source_rate == 0 {
        return Err("dictation sample rate must be non-zero".to_string());
    }
    let samples = resample_linear(samples, source_rate, 16_000);
    if should_skip_low_signal_audio(&samples) {
        return Ok(None);
    }
    Ok(Some(samples))
}

pub fn transcribe_dictation_recording<T: Transcriber>(
    recording: &DictationRecording,
    model_id: &str,
    language: TranscriptionLanguage,
    transcriber: T,
) -> Result<Option<String>, String> {
    let Some(samples) = prepare_dictation_audio(&recording.samples, recording.sample_rate)? else {
        return Ok(None);
    };
    let path = next_temp_wav_path();
    write_dictation_wav(&path, &samples)?;
    let result = transcriber
        .transcribe(TranscriptionRequest {
            audio_path: &path,
            model_id,
            language,
        })
        .map_err(|error| error.to_string())
        .map(|text| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        });
    let _ = std::fs::remove_file(&path);
    result
}

fn next_temp_wav_path() -> PathBuf {
    let id = NEXT_TEMP_WAV_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "wakenote-dictation-{}-{id}.wav",
        std::process::id()
    ))
}

fn write_dictation_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 16_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).map_err(|error| error.to_string())?;
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        writer
            .write_sample(value)
            .map_err(|error| error.to_string())?;
    }
    writer.finalize().map_err(|error| error.to_string())
}
