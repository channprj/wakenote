use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;

use crate::live_capture::{AudioFrame, AudioInputBackend, AudioInputConfig, LiveCaptureRuntime};
use crate::recorder::{ChunkSource, RecordedChunk, Recorder, RecordingRequest};
use crate::settings::{AppSettings, SettingsPatch, TranscriptionLanguage, expand_user_path};
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
    Error,
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
    PlayStopCue,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationShortcutEvent {
    Pressed,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutRegistrationChange {
    Unchanged,
    Register(String),
    Unregister(String),
    Replace { previous: String, next: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalModifierKey {
    LeftControl,
    RightControl,
    LeftAlt,
    RightAlt,
    LeftShift,
    RightShift,
    LeftCommand,
    RightCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierShortcut {
    Chord(u8),
    Physical(PhysicalModifierKey),
}

impl ModifierShortcut {
    const CONTROL: u8 = 1 << 0;
    const ALT: u8 = 1 << 1;
    const SHIFT: u8 = 1 << 2;
    const COMMAND: u8 = 1 << 3;

    pub const fn new(control: bool, alt: bool, shift: bool, command: bool) -> Self {
        Self::Chord(
            (if control { Self::CONTROL } else { 0 })
                | (if alt { Self::ALT } else { 0 })
                | (if shift { Self::SHIFT } else { 0 })
                | (if command { Self::COMMAND } else { 0 }),
        )
    }

    fn contains(self, required: Self) -> bool {
        match (self, required) {
            (Self::Chord(pressed), Self::Chord(required)) => pressed & required == required,
            (Self::Physical(pressed), Self::Physical(required)) => pressed == required,
            _ => false,
        }
    }

    pub fn canonical_string(self) -> Option<String> {
        if let Self::Physical(key) = self {
            return Some(key.canonical_name().to_string());
        }
        let Self::Chord(bits) = self else {
            unreachable!()
        };
        let mut parts = Vec::new();
        for (mask, name) in [
            (Self::CONTROL, "ctrl"),
            (Self::ALT, "alt"),
            (Self::SHIFT, "shift"),
            (Self::COMMAND, "cmd"),
        ] {
            if bits & mask != 0 {
                parts.push(name);
            }
        }
        (parts.len() >= 2).then(|| parts.join("+"))
    }
}

impl PhysicalModifierKey {
    fn from_canonical_name(raw: &str) -> Option<Self> {
        match raw {
            "leftctrl" => Some(Self::LeftControl),
            "rightctrl" => Some(Self::RightControl),
            "leftalt" => Some(Self::LeftAlt),
            "rightalt" => Some(Self::RightAlt),
            "leftshift" => Some(Self::LeftShift),
            "rightshift" => Some(Self::RightShift),
            "leftcmd" => Some(Self::LeftCommand),
            "rightcmd" => Some(Self::RightCommand),
            _ => None,
        }
    }

    fn canonical_name(self) -> &'static str {
        match self {
            Self::LeftControl => "leftctrl",
            Self::RightControl => "rightctrl",
            Self::LeftAlt => "leftalt",
            Self::RightAlt => "rightalt",
            Self::LeftShift => "leftshift",
            Self::RightShift => "rightshift",
            Self::LeftCommand => "leftcmd",
            Self::RightCommand => "rightcmd",
        }
    }
}

#[derive(Debug, Default)]
pub struct ModifierShortcutRuntime {
    shortcut: Option<ModifierShortcut>,
    latched: bool,
    active: bool,
}

impl ModifierShortcutRuntime {
    pub fn register(&mut self, shortcut: ModifierShortcut) -> Result<(), String> {
        if self.shortcut.is_some() {
            return Err("a modifier-only dictation shortcut is already registered".to_string());
        }
        self.shortcut = Some(shortcut);
        self.latched = true;
        self.active = false;
        Ok(())
    }

    pub fn unregister(&mut self, shortcut: ModifierShortcut) {
        if self.shortcut == Some(shortcut) {
            self.shortcut = None;
            self.latched = false;
            self.active = false;
        }
    }

    pub fn is_registered(&self, shortcut: ModifierShortcut) -> bool {
        self.shortcut == Some(shortcut)
    }

    pub fn handle_modifiers(
        &mut self,
        pressed: ModifierShortcut,
    ) -> Option<DictationShortcutEvent> {
        let Some(shortcut) = self.shortcut else {
            return None;
        };
        if self.active {
            if pressed == shortcut {
                return None;
            }
            self.active = false;
            self.latched = true;
            return Some(DictationShortcutEvent::Released);
        };
        if !pressed.contains(shortcut) {
            self.latched = false;
            return None;
        }
        if self.latched {
            return None;
        }
        self.latched = true;
        if pressed != shortcut {
            return None;
        }
        self.active = true;
        Some(DictationShortcutEvent::Pressed)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictationRecording {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationProcessOutcome {
    Typed(String),
    NoSpeech,
}

pub struct DictationRuntime<B: AudioInputBackend> {
    capture: LiveCaptureRuntime<B>,
    stage: DictationStage,
    last_press: Option<Instant>,
    samples: Arc<Mutex<Vec<f32>>>,
    sample_rate: Option<u32>,
    started_at: Option<DateTime<Utc>>,
    next_recording_id: u64,
    active_recording_id: Option<u64>,
    stop_cue_armed: bool,
}

impl<B: AudioInputBackend> DictationRuntime<B> {
    pub fn new(backend: B) -> Self {
        Self {
            capture: LiveCaptureRuntime::new(backend),
            stage: DictationStage::Idle,
            last_press: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            sample_rate: None,
            started_at: None,
            next_recording_id: 1,
            active_recording_id: None,
            stop_cue_armed: false,
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
            DictationStage::Error => DictationAction::Ignore,
        }
    }

    pub fn handle_shortcut_event(&mut self, event: DictationShortcutEvent) -> DictationAction {
        match event {
            DictationShortcutEvent::Pressed if self.stage == DictationStage::Idle => {
                self.stage = DictationStage::Recording;
                self.stop_cue_armed = true;
                DictationAction::StartRecording
            }
            DictationShortcutEvent::Released if self.stop_cue_armed => {
                self.stop_cue_armed = false;
                if self.stage == DictationStage::Recording {
                    self.stage = DictationStage::Transcribing;
                    DictationAction::StopAndTranscribe
                } else {
                    DictationAction::PlayStopCue
                }
            }
            _ => DictationAction::Ignore,
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
        self.start_recording_with_frame_handler(config, |_| {})
    }

    pub fn start_recording_with_frame_handler<F>(
        &mut self,
        config: AudioInputConfig,
        on_frame: F,
    ) -> Result<u64, String>
    where
        F: Fn(&AudioFrame) + Send + Sync + 'static,
    {
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
                on_frame(&frame);
            })
            .map_err(|error| error.to_string())?;

        let recording_id = self.next_recording_id;
        self.next_recording_id = self.next_recording_id.saturating_add(1);
        self.active_recording_id = Some(recording_id);
        self.sample_rate = Some(sample_rate);
        self.started_at = Some(Utc::now());
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
        let ended_at = Utc::now();
        let started_at = self.started_at.take().unwrap_or_else(|| {
            ended_at
                - chrono::Duration::milliseconds(
                    (self
                        .samples
                        .lock()
                        .map(|samples| samples.len())
                        .unwrap_or_default() as i64
                        * 1_000)
                        / i64::from(sample_rate),
                )
        });
        let samples = std::mem::take(&mut *self.samples.lock().map_err(|error| error.to_string())?);
        Ok(DictationRecording {
            samples,
            sample_rate,
            started_at,
            ended_at,
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
        self.stop_cue_armed = false;
        self.stage = DictationStage::Transcribing;
        Some(self.stop_recording())
    }

    pub fn finish(&mut self) {
        self.capture.stop();
        self.stage = DictationStage::Idle;
        self.sample_rate = None;
        self.started_at = None;
        self.active_recording_id = None;
        if let Ok(mut samples) = self.samples.lock() {
            samples.clear();
        }
    }

    pub fn fail(&mut self) {
        self.capture.stop();
        self.stage = DictationStage::Error;
        self.sample_rate = None;
        self.started_at = None;
        self.active_recording_id = None;
        if let Ok(mut samples) = self.samples.lock() {
            samples.clear();
        }
    }

    pub fn reset_error(&mut self) -> bool {
        if self.stage != DictationStage::Error {
            return false;
        }
        self.stage = DictationStage::Idle;
        true
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
    let parts = raw
        .split('+')
        .map(|part| part.trim().to_lowercase())
        .collect::<Vec<_>>();
    if parts.is_empty() || parts.iter().any(String::is_empty) {
        return Err("dictation shortcut cannot be empty".to_string());
    }

    if parts
        .iter()
        .any(|part| matches!(part.as_str(), "fn" | "function"))
    {
        return Err("the fn key is not supported for dictation shortcuts".to_string());
    }
    if parts.len() == 1
        && let Some(physical_modifier) = canonical_physical_modifier(&parts[0])
    {
        return Ok(physical_modifier.to_string());
    }
    if parts
        .iter()
        .any(|part| canonical_physical_modifier(part).is_some())
    {
        return Err("a physical modifier shortcut must be used alone".to_string());
    }

    let mut modifiers = Vec::new();
    let mut main_keys = Vec::new();
    for part in parts {
        let modifier = match part.as_str() {
            "ctrl" | "control" => Some("ctrl"),
            "alt" | "option" => Some("alt"),
            "shift" => Some("shift"),
            "cmd" | "command" | "meta" | "super" => Some("cmd"),
            _ => None,
        };
        if let Some(modifier) = modifier {
            if modifiers.contains(&modifier) {
                return Err(format!("duplicate dictation shortcut modifier: {modifier}"));
            }
            modifiers.push(modifier);
        } else {
            main_keys.push(part);
        }
    }
    modifiers.sort_by_key(|modifier| match *modifier {
        "ctrl" => 0,
        "alt" => 1,
        "shift" => 2,
        "cmd" => 3,
        _ => unreachable!(),
    });

    if main_keys.is_empty() && modifiers.len() >= 2 {
        return Ok(modifiers.join("+"));
    }
    if main_keys.len() != 1 {
        return Err("dictation shortcut must contain exactly one main key".to_string());
    }
    let main_key = main_keys[0].as_str();
    let normalized = modifiers
        .into_iter()
        .chain(std::iter::once(main_key))
        .collect::<Vec<_>>()
        .join("+");
    normalized
        .parse::<Shortcut>()
        .map_err(|error| format!("invalid dictation shortcut: {error}"))?;
    Ok(normalized)
}

fn canonical_physical_modifier(raw: &str) -> Option<&'static str> {
    match raw {
        "leftctrl" | "leftcontrol" | "ctrlleft" | "controlleft" => Some("leftctrl"),
        "rightctrl" | "rightcontrol" | "ctrlright" | "controlright" => Some("rightctrl"),
        "leftalt" | "leftoption" | "altleft" | "optionleft" => Some("leftalt"),
        "rightalt" | "rightoption" | "altright" | "optionright" => Some("rightalt"),
        "leftshift" | "shiftleft" => Some("leftshift"),
        "rightshift" | "shiftright" => Some("rightshift"),
        "leftcmd" | "leftcommand" | "cmdleft" | "commandleft" | "leftmeta" | "metaleft" => {
            Some("leftcmd")
        }
        "rightcmd" | "rightcommand" | "cmdright" | "commandright" | "rightmeta" | "metaright" => {
            Some("rightcmd")
        }
        _ => None,
    }
}

pub fn modifier_shortcut(raw: &str) -> Result<Option<ModifierShortcut>, String> {
    let normalized = validate_dictation_shortcut(raw)?;
    if let Some(key) = PhysicalModifierKey::from_canonical_name(&normalized) {
        return Ok(Some(ModifierShortcut::Physical(key)));
    }
    if normalized
        .split('+')
        .any(|part| !matches!(part, "ctrl" | "alt" | "shift" | "cmd"))
    {
        return Ok(None);
    }

    Ok(Some(ModifierShortcut::new(
        normalized.split('+').any(|part| part == "ctrl"),
        normalized.split('+').any(|part| part == "alt"),
        normalized.split('+').any(|part| part == "shift"),
        normalized.split('+').any(|part| part == "cmd"),
    )))
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

pub fn archive_dictation_recording(
    recording: &DictationRecording,
    settings: &AppSettings,
    device_id: &str,
    device_name: &str,
    used_fallback_device: bool,
    app_version: &str,
) -> Result<RecordedChunk, String> {
    let save_root = expand_user_path(&settings.save_root);
    let mut archive_settings = settings.clone();
    archive_settings.selected_model = settings.effective_dictation_model().to_string();
    Recorder::write_chunk(RecordingRequest {
        save_root: &save_root,
        settings: &archive_settings,
        samples: &recording.samples,
        sample_rate: recording.sample_rate,
        started_at: recording.started_at,
        ended_at: recording.ended_at,
        device_id,
        device_name,
        used_fallback_device,
        transcription_enabled: true,
        app_version,
        live_capture_chunk_id: None,
        source: ChunkSource::Microphone,
        source_label: Some("dictation"),
    })
    .map_err(|error| error.to_string())
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

pub fn transcribe_and_type_dictation_recording<T, F>(
    recording: &DictationRecording,
    model_id: &str,
    language: TranscriptionLanguage,
    transcriber: T,
    type_text: F,
) -> Result<DictationProcessOutcome, String>
where
    T: Transcriber,
    F: FnOnce(&str) -> Result<(), String>,
{
    let Some(text) = transcribe_dictation_recording(recording, model_id, language, transcriber)?
    else {
        return Ok(DictationProcessOutcome::NoSpeech);
    };
    type_text(&text)?;
    Ok(DictationProcessOutcome::Typed(text))
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
