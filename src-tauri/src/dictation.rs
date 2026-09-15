use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;
use tauri_plugin_global_shortcut::Shortcut;

use crate::dictionary::DictionaryContext;
use crate::live_capture::{AudioFrame, AudioInputBackend, AudioInputConfig};
use crate::multi_capture::{MicrophoneMixMode, MicrophoneMixer, MultiCaptureRuntime};
use crate::recorder::{ChunkSource, RecordedChunk, Recorder, RecordingRequest};
use crate::settings::{
    AppSettings, CaptureMicrophoneEntry, MicrophoneSlot, SettingsPatch, TranscriptionLanguage,
    expand_user_path,
};
use crate::transcription::{
    Transcriber, TranscriptionExecution, TranscriptionRequest, resample_linear,
    should_skip_low_signal_audio, should_suppress_transcript_artifact,
};
use crate::voice_leveling::AudioFrameProcessor;

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
    Cancel,
    PlayStopCue,
    Ignore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationShortcutEvent {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictationHotkeyMode {
    Hold,
    Toggle,
    Enhanced,
}

impl DictationHotkeyMode {
    fn index(self) -> usize {
        match self {
            Self::Hold => 0,
            Self::Toggle => 1,
            Self::Enhanced => 2,
        }
    }
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
        let shortcut = self.shortcut?;
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

/// Each modifier hotkey keeps its own latch so hold and toggle can coexist.
#[derive(Debug, Default)]
pub struct DictationModifierShortcuts {
    shortcuts: Vec<(DictationHotkeyMode, ModifierShortcutRuntime)>,
}

impl DictationModifierShortcuts {
    pub fn register(
        &mut self,
        shortcut: ModifierShortcut,
        mode: DictationHotkeyMode,
    ) -> Result<(), String> {
        if self.is_registered(shortcut) {
            return Err("this modifier hotkey is already registered".into());
        }
        let mut runtime = ModifierShortcutRuntime::default();
        runtime.register(shortcut)?;
        self.shortcuts.push((mode, runtime));
        Ok(())
    }

    pub fn unregister(&mut self, shortcut: ModifierShortcut) {
        self.shortcuts
            .retain(|(_, runtime)| !runtime.is_registered(shortcut));
    }

    pub fn is_registered(&self, shortcut: ModifierShortcut) -> bool {
        self.shortcuts
            .iter()
            .any(|(_, runtime)| runtime.is_registered(shortcut))
    }

    pub fn handle_modifiers(
        &mut self,
        pressed: ModifierShortcut,
    ) -> Vec<(DictationShortcutEvent, DictationHotkeyMode)> {
        self.shortcuts
            .iter_mut()
            .filter_map(|(mode, runtime)| {
                runtime
                    .handle_modifiers(pressed)
                    .map(|event| (event, *mode))
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationMicrophoneInput {
    pub slot: MicrophoneSlot,
    pub device: CaptureMicrophoneEntry,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationSlotFailure {
    pub slot: MicrophoneSlot,
    pub error: String,
}

#[derive(Debug)]
pub struct DictationMixSession {
    mixer: MicrophoneMixer,
    active_slots: BTreeSet<MicrophoneSlot>,
    started_inputs: BTreeMap<MicrophoneSlot, CaptureMicrophoneEntry>,
    failures: BTreeMap<MicrophoneSlot, String>,
    finished: bool,
}

impl DictationMixSession {
    pub fn new(
        base_time: DateTime<Utc>,
        target_sample_rate: u32,
        expected_slots: impl IntoIterator<Item = MicrophoneSlot>,
    ) -> Self {
        Self {
            mixer: MicrophoneMixer::with_mode(
                target_sample_rate,
                base_time,
                MicrophoneMixMode::Priority,
            ),
            active_slots: expected_slots.into_iter().collect(),
            started_inputs: BTreeMap::new(),
            failures: BTreeMap::new(),
            finished: false,
        }
    }

    pub fn mark_started(&mut self, input: DictationMicrophoneInput) {
        if self.finished {
            return;
        }
        self.active_slots.insert(input.slot);
        self.failures.remove(&input.slot);
        self.started_inputs.insert(input.slot, input.device);
    }

    pub fn mark_failed(&mut self, slot: MicrophoneSlot, error: String) -> Vec<AudioFrame> {
        if self.finished {
            return Vec::new();
        }
        self.active_slots.remove(&slot);
        self.failures.entry(slot).or_insert(error);
        let active_slots = self.active_slots();
        self.mixer.drain(&active_slots)
    }

    pub fn push_frame(
        &mut self,
        slot: MicrophoneSlot,
        input_sample_rate: u32,
        frame: AudioFrame,
    ) -> Vec<AudioFrame> {
        if self.finished || !self.active_slots.contains(&slot) {
            return Vec::new();
        }
        let active_slots = self.active_slots();
        self.mixer
            .push_frame(slot, input_sample_rate, frame, &active_slots)
    }

    pub fn finish(&mut self) -> Vec<AudioFrame> {
        if self.finished {
            return Vec::new();
        }
        self.finished = true;
        self.mixer.flush()
    }

    pub fn active_slots(&self) -> Vec<MicrophoneSlot> {
        self.active_slots.iter().copied().collect()
    }

    pub fn started_inputs(&self) -> Vec<DictationMicrophoneInput> {
        self.started_inputs
            .iter()
            .map(|(slot, device)| DictationMicrophoneInput {
                slot: *slot,
                device: device.clone(),
            })
            .collect()
    }

    pub fn failures(&self) -> Vec<DictationSlotFailure> {
        self.failures
            .iter()
            .map(|(slot, error)| DictationSlotFailure {
                slot: *slot,
                error: error.clone(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictationRecording {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub microphone_inputs: Vec<DictationMicrophoneInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictationProcessOutcome {
    Typed(String),
    NoSpeech,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictationCaptureResult {
    pub operation_id: u64,
    pub settings: AppSettings,
    pub recording: DictationRecording,
    pub failures: Vec<DictationSlotFailure>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DictationCaptureHealth {
    Healthy,
    Degraded {
        active_slots: Vec<MicrophoneSlot>,
        failures: Vec<DictationSlotFailure>,
    },
    Exhausted(Box<DictationCaptureResult>),
}

pub struct DictationRuntime<B: AudioInputBackend + Default> {
    capture: MultiCaptureRuntime<B>,
    mix_session: Option<Arc<Mutex<DictationMixSession>>>,
    stage: DictationStage,
    last_press: Option<Instant>,
    samples: Arc<Mutex<Vec<f32>>>,
    settings_snapshot: Option<AppSettings>,
    started_at: Option<DateTime<Utc>>,
    next_recording_id: u64,
    active_recording_id: Option<u64>,
    capture_claimed: bool,
    stop_cue_armed: bool,
    hotkey_pressed: [bool; 3],
    active_hotkey: Option<DictationHotkeyMode>,
}

impl<B: AudioInputBackend + Default> Default for DictationRuntime<B> {
    fn default() -> Self {
        Self {
            capture: MultiCaptureRuntime::default(),
            mix_session: None,
            stage: DictationStage::Idle,
            last_press: None,
            samples: Arc::new(Mutex::new(Vec::new())),
            settings_snapshot: None,
            started_at: None,
            next_recording_id: 1,
            active_recording_id: None,
            capture_claimed: false,
            stop_cue_armed: false,
            hotkey_pressed: [false; 3],
            active_hotkey: None,
        }
    }
}

impl<B: AudioInputBackend + Default> DictationRuntime<B> {
    pub fn reset_hotkeys(&mut self) {
        self.hotkey_pressed = [false; 3];
        self.active_hotkey = None;
    }

    pub fn handle_hotkey_event(
        &mut self,
        event: DictationShortcutEvent,
        mode: DictationHotkeyMode,
    ) -> DictationAction {
        let pressed = event == DictationShortcutEvent::Pressed;
        let was_pressed = &mut self.hotkey_pressed[mode.index()];
        if *was_pressed == pressed {
            return DictationAction::Ignore;
        }
        *was_pressed = pressed;

        // Releasing a hold key must never stop a recording started by toggle,
        // and an unrelated hotkey must not take over an active recording.
        if self.stage == DictationStage::Recording && self.active_hotkey != Some(mode) {
            return DictationAction::Ignore;
        }
        let action = if mode == DictationHotkeyMode::Toggle {
            if !pressed {
                return DictationAction::Ignore;
            }
            match self.stage {
                DictationStage::Idle => {
                    self.stage = DictationStage::Recording;
                    DictationAction::StartRecording
                }
                DictationStage::Recording => {
                    self.stage = DictationStage::Transcribing;
                    DictationAction::StopAndTranscribe
                }
                DictationStage::Transcribing => DictationAction::Cancel,
                DictationStage::Error => DictationAction::Ignore,
            }
        } else {
            self.handle_shortcut_event(event)
        };
        if action == DictationAction::StartRecording {
            self.active_hotkey = Some(mode);
        }
        action
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
            DictationStage::Transcribing => DictationAction::Cancel,
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
            DictationShortcutEvent::Pressed if self.stage == DictationStage::Transcribing => {
                DictationAction::Cancel
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
        mut on_frame: F,
    ) -> Result<u64, String>
    where
        F: FnMut(&AudioFrame) + Send + 'static,
    {
        let input = DictationMicrophoneInput {
            slot: MicrophoneSlot::Primary,
            device: CaptureMicrophoneEntry {
                id: config.device_id.clone(),
                label: config
                    .label_hint
                    .clone()
                    .unwrap_or_else(|| config.device_id.clone()),
                core_audio_uid: config.core_audio_uid.clone(),
            },
        };
        let operation_id = self.begin_recording(
            AppSettings::default(),
            Utc::now(),
            [MicrophoneSlot::Primary],
        )?;
        if let Err(error) =
            self.start_slot_with_frame_handler(operation_id, input, config, move |frame| {
                on_frame(frame)
            })
        {
            self.mark_slot_start_failed(operation_id, MicrophoneSlot::Primary, error.clone());
            return Err(error);
        }
        Ok(operation_id)
    }

    pub fn start_recording_with_processor<P, F>(
        &mut self,
        settings: AppSettings,
        input: DictationMicrophoneInput,
        config: AudioInputConfig,
        processor: P,
        on_frame: F,
    ) -> Result<u64, String>
    where
        P: AudioFrameProcessor + 'static,
        F: FnMut(&AudioFrame) + Send + 'static,
    {
        let slot = input.slot;
        let operation_id = self.begin_recording(settings, Utc::now(), [slot])?;
        if let Err(error) =
            self.start_slot_with_processor(operation_id, input, config, processor, on_frame)
        {
            self.mark_slot_start_failed(operation_id, slot, error.clone());
            return Err(error);
        }
        Ok(operation_id)
    }

    pub fn begin_recording(
        &mut self,
        settings: AppSettings,
        base_time: DateTime<Utc>,
        expected_slots: impl IntoIterator<Item = MicrophoneSlot>,
    ) -> Result<u64, String> {
        if self.stage != DictationStage::Recording {
            return Err("dictation is not ready to record".to_string());
        }
        if self.active_recording_id.is_some() {
            return Err("dictation capture is already active".to_string());
        }
        let expected_slots = expected_slots.into_iter().collect::<Vec<_>>();
        if expected_slots.is_empty() {
            return Err("dictation requires at least one microphone slot".to_string());
        }
        self.capture.stop_all();
        self.samples
            .lock()
            .map_err(|error| error.to_string())?
            .clear();
        let operation_id = self.next_recording_id;
        self.next_recording_id = self.next_recording_id.saturating_add(1);
        self.mix_session = Some(Arc::new(Mutex::new(DictationMixSession::new(
            base_time,
            16_000,
            expected_slots,
        ))));
        self.settings_snapshot = Some(settings);
        self.started_at = Some(base_time);
        self.active_recording_id = Some(operation_id);
        self.capture_claimed = false;
        Ok(operation_id)
    }

    fn start_slot_with_frame_handler<F>(
        &mut self,
        operation_id: u64,
        input: DictationMicrophoneInput,
        config: AudioInputConfig,
        mut on_frame: F,
    ) -> Result<(), String>
    where
        F: FnMut(&AudioFrame) + Send + 'static,
    {
        if !self.capture_is_open(operation_id) {
            return Err("dictation operation is unavailable".to_string());
        }
        let sample_rate = config
            .sample_rate
            .filter(|sample_rate| *sample_rate > 0)
            .ok_or_else(|| "dictation requires a valid sample rate".to_string())?;
        let slot = input.slot;
        let mix_session = self
            .mix_session
            .as_ref()
            .cloned()
            .ok_or_else(|| "dictation mix session is unavailable".to_string())?;
        let samples = self.samples.clone();
        self.capture
            .start_slot(slot, config, move |frame| {
                let mixed = mix_session
                    .lock()
                    .map(|mut session| session.push_frame(slot, sample_rate, frame))
                    .unwrap_or_default();
                for frame in mixed {
                    if let Ok(mut samples) = samples.lock() {
                        samples.extend_from_slice(&frame.samples);
                    }
                    on_frame(&frame);
                }
            })
            .map_err(|error| error.to_string())?;
        self.mark_slot_started(operation_id, input)
    }

    pub fn start_slot_with_processor<P, F>(
        &mut self,
        operation_id: u64,
        input: DictationMicrophoneInput,
        config: AudioInputConfig,
        processor: P,
        mut on_frame: F,
    ) -> Result<(), String>
    where
        P: AudioFrameProcessor + 'static,
        F: FnMut(&AudioFrame) + Send + 'static,
    {
        if !self.capture_is_open(operation_id) {
            return Err("dictation operation is unavailable".to_string());
        }
        let sample_rate = config
            .sample_rate
            .filter(|sample_rate| *sample_rate > 0)
            .ok_or_else(|| "dictation requires a valid sample rate".to_string())?;
        let slot = input.slot;
        let mix_session = self
            .mix_session
            .as_ref()
            .cloned()
            .ok_or_else(|| "dictation mix session is unavailable".to_string())?;
        let samples = self.samples.clone();
        self.capture
            .start_slot_processed(slot, config, processor, move |frame| {
                let mixed = mix_session
                    .lock()
                    .map(|mut session| session.push_frame(slot, sample_rate, frame))
                    .unwrap_or_default();
                for frame in mixed {
                    if let Ok(mut samples) = samples.lock() {
                        samples.extend_from_slice(&frame.samples);
                    }
                    on_frame(&frame);
                }
            })
            .map_err(|error| error.to_string())?;
        self.mark_slot_started(operation_id, input)
    }

    fn mark_slot_started(
        &mut self,
        operation_id: u64,
        input: DictationMicrophoneInput,
    ) -> Result<(), String> {
        if !self.capture_is_open(operation_id) {
            return Err("dictation operation is unavailable".to_string());
        }
        self.mix_session
            .as_ref()
            .ok_or_else(|| "dictation mix session is unavailable".to_string())?
            .lock()
            .map_err(|error| error.to_string())?
            .mark_started(input);
        Ok(())
    }

    pub fn mark_slot_start_failed(
        &mut self,
        operation_id: u64,
        slot: MicrophoneSlot,
        error: String,
    ) -> bool {
        if !self.capture_is_open(operation_id) {
            return false;
        }
        self.capture.stop_slot(slot);
        let Some(session) = self.mix_session.as_ref() else {
            return false;
        };
        let Ok(mut session) = session.lock() else {
            return false;
        };
        let drained = session.mark_failed(slot, error);
        drop(session);
        self.append_frames(drained);
        true
    }

    pub fn active_slots(&self, operation_id: u64) -> Vec<MicrophoneSlot> {
        if !self.capture_is_open(operation_id) {
            return Vec::new();
        }
        self.mix_session
            .as_ref()
            .and_then(|session| session.lock().ok().map(|session| session.active_slots()))
            .unwrap_or_default()
    }

    pub fn poll_capture_health(&mut self, operation_id: u64) -> Option<DictationCaptureHealth> {
        if !self.capture_is_open(operation_id) {
            return None;
        }
        let known_failures = self
            .mix_session
            .as_ref()?
            .lock()
            .ok()?
            .failures()
            .into_iter()
            .map(|failure| failure.slot)
            .collect::<BTreeSet<_>>();
        let new_failures = self
            .capture
            .diagnostics()
            .into_iter()
            .filter_map(|diagnostic| {
                (!known_failures.contains(&diagnostic.slot))
                    .then_some((diagnostic.slot, diagnostic.runtime_error?))
            })
            .collect::<Vec<_>>();
        if new_failures.is_empty() {
            return Some(DictationCaptureHealth::Healthy);
        }
        for (slot, error) in new_failures {
            self.mark_slot_start_failed(operation_id, slot, error);
        }
        let active_slots = self.active_slots(operation_id);
        let failures = self.mix_session.as_ref()?.lock().ok()?.failures();
        if active_slots.is_empty() {
            self.stage = DictationStage::Transcribing;
            self.take_capture_result(operation_id, DictationStage::Transcribing)
                .and_then(Result::ok)
                .map(Box::new)
                .map(DictationCaptureHealth::Exhausted)
        } else {
            Some(DictationCaptureHealth::Degraded {
                active_slots,
                failures,
            })
        }
    }

    pub fn stop_recording(&mut self) -> Result<DictationCaptureResult, String> {
        if self.stage != DictationStage::Transcribing {
            return Err("dictation is not ready to transcribe".to_string());
        }
        let operation_id = self
            .active_recording_id
            .ok_or_else(|| "dictation operation is unavailable".to_string())?;
        self.take_capture_result(operation_id, DictationStage::Transcribing)
            .ok_or_else(|| "dictation capture was already finalized".to_string())?
    }

    pub fn stop_if_recording(
        &mut self,
        operation_id: u64,
    ) -> Option<Result<DictationCaptureResult, String>> {
        if self.stage != DictationStage::Recording
            || self.active_recording_id != Some(operation_id)
            || self.capture_claimed
        {
            return None;
        }
        self.stop_cue_armed = false;
        self.stage = DictationStage::Transcribing;
        self.take_capture_result(operation_id, DictationStage::Transcribing)
    }

    fn take_capture_result(
        &mut self,
        operation_id: u64,
        next_stage: DictationStage,
    ) -> Option<Result<DictationCaptureResult, String>> {
        if self.active_recording_id != Some(operation_id) || self.capture_claimed {
            return None;
        }
        self.capture_claimed = true;
        self.stop_cue_armed = false;
        self.stage = next_stage;
        self.capture.stop_all();
        let session = match self.mix_session.take() {
            Some(session) => session,
            None => return Some(Err("dictation mix session is unavailable".to_string())),
        };
        let (tail, microphone_inputs, failures) = match session.lock() {
            Ok(mut session) => (
                session.finish(),
                session.started_inputs(),
                session.failures(),
            ),
            Err(error) => return Some(Err(error.to_string())),
        };
        self.append_frames(tail);
        Some(self.build_capture_result(operation_id, microphone_inputs, failures))
    }

    fn build_capture_result(
        &mut self,
        operation_id: u64,
        microphone_inputs: Vec<DictationMicrophoneInput>,
        failures: Vec<DictationSlotFailure>,
    ) -> Result<DictationCaptureResult, String> {
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
                        / 16_000,
                )
        });
        let samples = std::mem::take(&mut *self.samples.lock().map_err(|error| error.to_string())?);
        let settings = self
            .settings_snapshot
            .clone()
            .ok_or_else(|| "dictation settings snapshot is unavailable".to_string())?;
        Ok(DictationCaptureResult {
            operation_id,
            settings,
            recording: DictationRecording {
                samples,
                sample_rate: 16_000,
                started_at,
                ended_at,
                microphone_inputs,
            },
            failures,
        })
    }

    fn append_frames(&self, frames: Vec<AudioFrame>) {
        if let Ok(mut samples) = self.samples.lock() {
            for frame in frames {
                samples.extend_from_slice(&frame.samples);
            }
        }
    }

    fn capture_is_open(&self, operation_id: u64) -> bool {
        self.active_recording_id == Some(operation_id)
            && self.stage == DictationStage::Recording
            && !self.capture_claimed
    }

    pub fn finish(&mut self) {
        self.capture.stop_all();
        self.stage = DictationStage::Idle;
        self.stop_cue_armed = false;
        self.clear_capture_state();
    }

    pub fn current_operation_id(&self) -> Option<u64> {
        self.active_recording_id
    }

    pub fn is_operation_active(&self, operation_id: u64) -> bool {
        self.active_recording_id == Some(operation_id)
            && matches!(
                self.stage,
                DictationStage::Recording | DictationStage::Transcribing
            )
    }

    pub fn finish_if_active(&mut self, operation_id: u64) -> bool {
        if !self.is_operation_active(operation_id) {
            return false;
        }
        self.finish();
        true
    }

    pub fn fail_if_active(&mut self, operation_id: u64) -> bool {
        if !self.is_operation_active(operation_id) {
            return false;
        }
        self.fail();
        true
    }

    pub fn cancel_active(&mut self) -> bool {
        if !matches!(
            self.stage,
            DictationStage::Recording | DictationStage::Transcribing
        ) {
            return false;
        }
        self.stop_cue_armed = false;
        self.finish();
        true
    }

    pub fn fail(&mut self) {
        self.capture.stop_all();
        self.stage = DictationStage::Error;
        self.clear_capture_state();
    }

    fn clear_capture_state(&mut self) {
        self.active_hotkey = None;
        self.mix_session = None;
        self.settings_snapshot = None;
        self.started_at = None;
        self.active_recording_id = None;
        self.capture_claimed = false;
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
        if candidate.enhance_prompt_enabled {
            let enhanced = validate_dictation_shortcut(&candidate.enhance_prompt_shortcut)?;
            if modifier_shortcut(&enhanced)?.is_some() {
                return Err(
                    "Enhanced Prompt requires a shortcut with a main key, such as Ctrl+Alt+Space"
                        .into(),
                );
            }
        }
        let shortcuts = active_dictation_shortcuts(&candidate);
        for (index, (shortcut, mode)) in shortcuts.iter().enumerate() {
            validate_dictation_shortcut(shortcut)?;
            for (other, other_mode) in &shortcuts[..index] {
                if dictation_shortcuts_overlap(shortcut, other)? {
                    return Err(format!(
                        "{} overlaps {}. Choose a different key combination.",
                        hotkey_label(*mode),
                        hotkey_label(*other_mode),
                    ));
                }
            }
        }
    }
    Ok(candidate)
}

fn hotkey_label(mode: DictationHotkeyMode) -> &'static str {
    match mode {
        DictationHotkeyMode::Hold => "Hold-to-dictate hotkey",
        DictationHotkeyMode::Toggle => "Toggle dictation hotkey",
        DictationHotkeyMode::Enhanced => "Enhanced Prompt shortcut",
    }
}

pub fn active_dictation_shortcuts(settings: &AppSettings) -> Vec<(String, DictationHotkeyMode)> {
    if !settings.dictation_enabled {
        return Vec::new();
    }
    let mut shortcuts = vec![(
        settings.dictation_shortcut.clone(),
        DictationHotkeyMode::Hold,
    )];
    if !settings.dictation_toggle_shortcut.is_empty() {
        shortcuts.push((
            settings.dictation_toggle_shortcut.clone(),
            DictationHotkeyMode::Toggle,
        ));
    }
    if settings.enhance_prompt_enabled {
        shortcuts.push((
            settings.enhance_prompt_shortcut.clone(),
            DictationHotkeyMode::Enhanced,
        ));
    }
    shortcuts
}

pub fn dictation_shortcuts_overlap(left: &str, right: &str) -> Result<bool, String> {
    let left = validate_dictation_shortcut(left)?;
    let right = validate_dictation_shortcut(right)?;
    if left == right {
        return Ok(true);
    }
    // Opposite physical keys can be assigned independently.
    if matches!(
        modifier_shortcut(&left)?,
        Some(ModifierShortcut::Physical(_))
    ) && matches!(
        modifier_shortcut(&right)?,
        Some(ModifierShortcut::Physical(_))
    ) {
        return Ok(false);
    }
    for (required, other) in [(&left, &right), (&right, &left)] {
        if modifier_shortcut(required)?.is_some() {
            let other_parts = other.split('+').map(logical_modifier).collect::<Vec<_>>();
            if required
                .split('+')
                .map(logical_modifier)
                .all(|part| other_parts.contains(&part))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn logical_modifier(part: &str) -> &str {
    part.strip_prefix("left")
        .or_else(|| part.strip_prefix("right"))
        .unwrap_or(part)
}

pub fn normalize_dictation_patch(patch: &mut SettingsPatch) -> Result<(), String> {
    if let Some(shortcut) = patch.dictation_shortcut.as_deref() {
        patch.dictation_shortcut = Some(validate_dictation_shortcut(shortcut)?);
    }
    if let Some(shortcut) = patch.dictation_toggle_shortcut.as_deref() {
        patch.dictation_toggle_shortcut = Some(if shortcut.trim().is_empty() {
            String::new()
        } else {
            validate_dictation_shortcut(shortcut)?
        });
    }
    if let Some(shortcut) = patch.enhance_prompt_shortcut.as_deref() {
        let normalized = validate_dictation_shortcut(shortcut)?;
        if modifier_shortcut(&normalized)?.is_some() {
            return Err("Enhanced Prompt requires a shortcut with a main key".into());
        }
        patch.enhance_prompt_shortcut = Some(normalized);
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
    app_version: &str,
) -> Result<RecordedChunk, String> {
    let mut actual_inputs = recording.microphone_inputs.clone();
    actual_inputs.sort_by_key(|input| input.slot);
    let devices = actual_inputs
        .iter()
        .map(|input| input.device.clone())
        .collect::<Vec<_>>();
    if devices.is_empty() {
        return Err("dictation recording has no captured microphone".to_string());
    }
    let device_id = devices
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<Vec<_>>()
        .join("+");
    let device_name = devices
        .iter()
        .map(|entry| entry.label.as_str())
        .collect::<Vec<_>>()
        .join(" + ");
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
        device_id: &device_id,
        device_name: &device_name,
        used_fallback_device: false,
        transcription_enabled: true,
        app_version,
        live_capture_chunk_id: None,
        source: ChunkSource::Microphone,
        source_label: Some("dictation"),
        microphone_inputs: Some(&devices),
    })
    .map_err(|error| error.to_string())
}

pub fn transcribe_dictation_recording<T: Transcriber>(
    recording: &DictationRecording,
    model_id: &str,
    language: TranscriptionLanguage,
    suppress_low_confidence_transcripts: bool,
    dictionary: &DictionaryContext,
    transcriber: T,
) -> Result<Option<String>, String> {
    transcribe_dictation_recording_execution(
        recording,
        model_id,
        language,
        &[language],
        suppress_low_confidence_transcripts,
        dictionary,
        transcriber,
    )
    .map(|execution| execution.map(|execution| execution.text))
}

pub fn transcribe_dictation_recording_execution<T: Transcriber>(
    recording: &DictationRecording,
    model_id: &str,
    language: TranscriptionLanguage,
    language_hints: &[TranscriptionLanguage],
    suppress_low_confidence_transcripts: bool,
    dictionary: &DictionaryContext,
    transcriber: T,
) -> Result<Option<TranscriptionExecution>, String> {
    let Some(samples) = prepare_dictation_audio(&recording.samples, recording.sample_rate)? else {
        return Ok(None);
    };
    let path = next_temp_wav_path();
    write_dictation_wav(&path, &samples)?;
    let result = transcriber
        .transcribe_execution(TranscriptionRequest {
            audio_path: &path,
            model_id,
            language,
            language_hints,
            dictionary,
        })
        .map_err(|error| error.to_string())
        .map(|mut execution| {
            execution.text = dictionary.correct(&execution.text);
            if suppress_low_confidence_transcripts
                && should_suppress_transcript_artifact(&execution.text)
            {
                execution.text = String::new();
            }
            (!execution.text.trim().is_empty()).then_some(execution)
        });
    let _ = std::fs::remove_file(&path);
    result
}

pub fn transcribe_and_type_dictation_recording<T, F>(
    recording: &DictationRecording,
    model_id: &str,
    language: TranscriptionLanguage,
    suppress_low_confidence_transcripts: bool,
    dictionary: &DictionaryContext,
    transcriber: T,
    type_text: F,
) -> Result<DictationProcessOutcome, String>
where
    T: Transcriber,
    F: FnOnce(&str) -> Result<(), String>,
{
    let Some(text) = transcribe_dictation_recording(
        recording,
        model_id,
        language,
        suppress_low_confidence_transcripts,
        dictionary,
        transcriber,
    )?
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
