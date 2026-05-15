use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone, Utc};

use crate::audio::{
    InputDevice, LevelMonitor, LevelSnapshot, MicHealthAction, MicHealthMonitor, MicHealthVerdict,
    dbfs_from_samples, list_input_devices,
};
use crate::capture::{CaptureController, CaptureControllerConfig, CaptureControllerEvent};
use crate::live_capture::AudioFrame;
use crate::models::{ModelDescriptor, ModelStatus, ModelStore, default_model_registry};
use crate::persistence::{AppPersistence, PersistenceError};
use crate::queue::{BacklogScan, QueueSnapshot, TranscriptionQueue, is_importable_audio_path};
use crate::recorder::{ChunkMetadata, RecordedChunk, TranscriptionStatus};
use crate::settings::{AppSettings, SettingsPatch, TranscriptionLanguage, expand_user_path};
use crate::storage::copy_uploaded_audio_file;
use crate::transcription::{
    Transcriber, TranscriptionJobOutcome, TranscriptionWorker, TranscriptionWorkerOptions,
    WhisperTranscriber, apply_outcome,
};

/// How many recently committed chunk_ids we keep around for audio_path -> chunk_id
/// reverse-lookup after a queue worker finishes. Bounded to avoid unbounded growth
/// over a long session.
const LIVE_CHUNK_HISTORY_LIMIT: usize = 256;

/// Events surfaced from the capture pipeline that the runtime should forward
/// to the frontend (or to the live transcription service).
#[derive(Debug, Clone)]
pub enum LiveTranscriptEvent {
    Started {
        chunk_id: u64,
        started_at: DateTime<Utc>,
        overlay_position: crate::settings::FloatingOverlayPosition,
    },
    SamplesReady {
        chunk_id: u64,
        model_id: String,
        language: TranscriptionLanguage,
        suppress_low_confidence_transcripts: bool,
        sample_rate: u32,
        samples: Arc<Vec<f32>>,
    },
    Committed {
        chunk_id: u64,
        audio_path: PathBuf,
        overlay_position: crate::settings::FloatingOverlayPosition,
        will_transcribe: bool,
    },
}

pub type LiveEventHandler = Arc<dyn Fn(LiveTranscriptEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppMode {
    RecordingAndTranscription,
    RecordingOnly,
    TranscriptionOnly,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayState {
    Idle,
    Listening,
    Recording,
    Transcribing,
    Paused,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayPresentation {
    pub rgba: [u8; 4],
    pub tooltip: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayRuntimePresentation {
    pub icon: TrayPresentation,
    pub visible: bool,
}

pub fn tray_presentation_for_state(state: TrayState) -> TrayPresentation {
    match state {
        TrayState::Idle => TrayPresentation {
            rgba: [100, 116, 139, 255],
            tooltip: "WakeNote: Idle",
        },
        TrayState::Listening => TrayPresentation {
            rgba: [0, 0, 0, 255],
            tooltip: "WakeNote: Listening",
        },
        TrayState::Recording => TrayPresentation {
            rgba: [22, 163, 74, 255],
            tooltip: "WakeNote: Recording",
        },
        TrayState::Transcribing => TrayPresentation {
            rgba: [217, 119, 6, 255],
            tooltip: "WakeNote: Transcribing",
        },
        TrayState::Paused => TrayPresentation {
            rgba: [71, 85, 105, 255],
            tooltip: "WakeNote: Paused",
        },
        TrayState::Error => TrayPresentation {
            rgba: [220, 38, 38, 255],
            tooltip: "WakeNote: Error",
        },
    }
}

pub fn tray_runtime_presentation(
    settings: &AppSettings,
    status: &AppStatus,
) -> TrayRuntimePresentation {
    TrayRuntimePresentation {
        icon: tray_presentation_for_state(status.tray_state),
        visible: settings.show_tray_icon,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayMenuPresentation {
    pub recording_checked: bool,
    pub transcription_checked: bool,
    pub pause_all_checked: bool,
    pub active_model_text: String,
    pub active_microphone_text: String,
    pub threshold_text: String,
}

pub fn tray_menu_presentation(settings: &AppSettings, status: &AppStatus) -> TrayMenuPresentation {
    TrayMenuPresentation {
        recording_checked: settings.recording_enabled,
        transcription_checked: settings.transcription_enabled,
        pause_all_checked: settings.pause_all,
        active_model_text: format!("Model: {}", status.active_model),
        active_microphone_text: format!("Microphone: {}", status.active_microphone),
        threshold_text: format!("Threshold: {:.0} dBFS", status.threshold_dbfs),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevealSaveFolderRequest {
    pub program: PathBuf,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainWindowCloseAction {
    HideToTray,
    AllowClose,
}

pub fn main_window_close_action(window_label: &str) -> MainWindowCloseAction {
    if window_label == "main" {
        MainWindowCloseAction::HideToTray
    } else {
        MainWindowCloseAction::AllowClose
    }
}

pub fn reveal_save_folder_request(settings: &AppSettings) -> RevealSaveFolderRequest {
    RevealSaveFolderRequest {
        program: PathBuf::from("/usr/bin/open"),
        path: expand_user_path(&settings.save_root),
    }
}

pub fn audio_playback_content_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("m4a") => Some("audio/mp4"),
        Some("mp3") => Some("audio/mpeg"),
        Some("wav") => Some("audio/wav"),
        _ => None,
    }
}

pub fn validate_audio_playback_file(path: &Path) -> Result<&'static str, String> {
    let content_type = audio_playback_content_type(path)
        .ok_or_else(|| "only mp3, m4a, and wav recordings can be played".to_string())?;
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("recording path is not a file".to_string());
    }
    Ok(content_type)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrophoneDevice {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppStatus {
    pub mode: AppMode,
    pub tray_state: TrayState,
    pub live_input_active: bool,
    pub active_model: String,
    pub active_microphone: String,
    pub microphone_warning: Option<String>,
    pub runtime_warning: Option<String>,
    pub threshold_dbfs: f32,
    pub level: LevelSnapshot,
    pub queue: QueueSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentTranscript {
    pub transcript_path: String,
    pub audio_path: Option<String>,
    pub recorded_at: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadedAudio {
    pub audio_path: String,
    pub original_filename: String,
    pub stored_at: String,
}

#[derive(Debug, Clone)]
pub struct StartedTranscriptionJob {
    pub job: crate::queue::QueueJob,
    pub model_directory: std::path::PathBuf,
    pub language: TranscriptionLanguage,
    pub suppress_low_confidence_transcripts: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedModelDownload {
    pub model_directory: std::path::PathBuf,
    pub model: ModelDescriptor,
    pub registry: Vec<ModelDescriptor>,
}

pub struct AppBackend {
    settings: AppSettings,
    queue: TranscriptionQueue,
    capture: Option<CaptureController>,
    level_monitor: LevelMonitor,
    active_microphone_label: Option<String>,
    microphone_warning: Option<String>,
    persistence: Option<AppPersistence>,
    live_event_handler: Option<LiveEventHandler>,
    pending_live_events: Vec<LiveTranscriptEvent>,
    chunk_id_history: VecDeque<(PathBuf, u64)>,
    chunk_id_index: HashMap<PathBuf, u64>,
    mic_health: MicHealthMonitor,
    /// One-shot device-id override used by the recovery watchdog to force a
    /// restart onto a specific device (e.g. system default) on the next
    /// capture start. Consumed by `take_microphone_recovery_override`.
    mic_recovery_override: Option<String>,
}

impl std::fmt::Debug for AppBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppBackend")
            .field("settings", &self.settings)
            .field("queue", &self.queue)
            .field("capture", &self.capture)
            .field("level_monitor", &self.level_monitor)
            .field("active_microphone_label", &self.active_microphone_label)
            .field("microphone_warning", &self.microphone_warning)
            .field("persistence", &self.persistence)
            .field(
                "live_event_handler",
                &self.live_event_handler.as_ref().map(|_| "<handler>"),
            )
            .field("pending_live_events_len", &self.pending_live_events.len())
            .field("chunk_id_history_len", &self.chunk_id_history.len())
            .field("mic_health", &self.mic_health)
            .field("mic_recovery_override", &self.mic_recovery_override)
            .finish()
    }
}

impl Default for AppBackend {
    fn default() -> Self {
        Self {
            settings: AppSettings::default(),
            queue: TranscriptionQueue::new(),
            capture: None,
            level_monitor: LevelMonitor::default(),
            active_microphone_label: None,
            microphone_warning: None,
            persistence: None,
            live_event_handler: None,
            pending_live_events: Vec::new(),
            chunk_id_history: VecDeque::new(),
            chunk_id_index: HashMap::new(),
            mic_health: MicHealthMonitor::default(),
            mic_recovery_override: None,
        }
    }
}

impl AppBackend {
    pub fn load_from_dir(root: impl AsRef<std::path::Path>) -> Result<Self, PersistenceError> {
        let persistence = AppPersistence::new(root);
        Ok(Self {
            settings: persistence.load_settings()?.unwrap_or_default(),
            queue: persistence.load_queue()?.unwrap_or_default(),
            capture: None,
            level_monitor: LevelMonitor::default(),
            active_microphone_label: None,
            microphone_warning: None,
            persistence: Some(persistence),
            live_event_handler: None,
            pending_live_events: Vec::new(),
            chunk_id_history: VecDeque::new(),
            chunk_id_index: HashMap::new(),
            mic_health: MicHealthMonitor::default(),
            mic_recovery_override: None,
        })
    }

    pub fn set_live_event_handler(&mut self, handler: LiveEventHandler) {
        self.live_event_handler = Some(handler);
    }

    pub fn live_event_handler(&self) -> Option<LiveEventHandler> {
        self.live_event_handler.clone()
    }

    pub fn drain_live_events(&mut self) -> Vec<LiveTranscriptEvent> {
        std::mem::take(&mut self.pending_live_events)
    }

    pub fn chunk_id_for_audio_path(&self, audio_path: &std::path::Path) -> Option<u64> {
        self.chunk_id_index.get(audio_path).copied()
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.clone()
    }

    pub fn update_settings(&mut self, mut patch: SettingsPatch) -> AppSettings {
        if let Some(model_id) = patch.selected_model.as_deref() {
            let model_directory = patch
                .model_directory
                .as_deref()
                .unwrap_or(&self.settings.model_directory);
            if !model_is_selectable(model_id, model_directory) {
                patch.selected_model = None;
            }
        }
        self.settings.apply_patch(patch);
        self.sync_capture_settings();
        self.persist_settings();
        self.settings.clone()
    }

    fn save_root_path(&self) -> PathBuf {
        expand_user_path(&self.settings.save_root)
    }

    fn model_directory_path(&self) -> PathBuf {
        expand_user_path(&self.settings.model_directory)
    }

    fn model_store(&self) -> ModelStore {
        ModelStore::new(self.model_directory_path())
    }

    pub fn model_registry(&self) -> Vec<ModelDescriptor> {
        model_registry_snapshot(self.model_directory_path())
    }

    pub fn verify_model(&self, model_id: &str) -> Result<Vec<ModelDescriptor>, String> {
        let store = self.model_store();
        let registry = store
            .load_model_registry()
            .map_err(|error| error.to_string())?;
        if !registry.contains_key(model_id) {
            return Err(format!("unknown model {model_id}"));
        }

        Ok(self.model_registry())
    }

    pub fn download_model(&self, model_id: &str) -> Result<Vec<ModelDescriptor>, String> {
        let prepared = self.prepare_model_download(model_id)?;
        let store = ModelStore::new(prepared.model_directory);
        store
            .download_model(&prepared.model)
            .map_err(|error| error.to_string())?;
        Ok(self.model_registry())
    }

    pub fn prepare_model_download(&self, model_id: &str) -> Result<PreparedModelDownload, String> {
        let store = self.model_store();
        let registry = store
            .load_model_registry()
            .map_err(|error| error.to_string())?;
        let model = registry
            .get(model_id)
            .ok_or_else(|| format!("unknown model {model_id}"))?
            .clone();
        if model.download_url.is_none() {
            return Err(format!("model {model_id} has no download URL"));
        }
        let mut refreshed_models: Vec<ModelDescriptor> = registry.values().cloned().collect();
        store
            .refresh_statuses(&mut refreshed_models)
            .map_err(|error| error.to_string())?;
        let current_status = refreshed_models
            .iter()
            .find(|candidate| candidate.id == model_id)
            .map(|candidate| candidate.status)
            .unwrap_or(model.status);
        match current_status {
            ModelStatus::Downloading | ModelStatus::Verifying | ModelStatus::Extracting => {
                return Err(format!("model {model_id} download is already active"));
            }
            ModelStatus::Installed | ModelStatus::Ready | ModelStatus::Unloaded => {
                return Err(format!("model {model_id} is already installed"));
            }
            ModelStatus::Missing | ModelStatus::Error => {}
        }

        store
            .record_download_progress(
                &model.id,
                0,
                Some(model.size_mb.saturating_mul(1024 * 1024)),
            )
            .map_err(|error| error.to_string())?;

        Ok(PreparedModelDownload {
            model_directory: self.model_directory_path(),
            model,
            registry: self.model_registry(),
        })
    }

    pub fn cancel_model_download(&self, model_id: &str) -> Result<Vec<ModelDescriptor>, String> {
        let store = self.model_store();
        let registry = store
            .load_model_registry()
            .map_err(|error| error.to_string())?;
        if !registry.contains_key(model_id) {
            return Err(format!("unknown model {model_id}"));
        }
        let mut refreshed_models: Vec<ModelDescriptor> = registry.values().cloned().collect();
        store
            .refresh_statuses(&mut refreshed_models)
            .map_err(|error| error.to_string())?;
        let current_status = refreshed_models
            .iter()
            .find(|candidate| candidate.id == model_id)
            .map(|candidate| candidate.status)
            .unwrap_or(ModelStatus::Missing);
        if !matches!(
            current_status,
            ModelStatus::Downloading | ModelStatus::Verifying | ModelStatus::Extracting
        ) {
            return Err(format!("model {model_id} has no active download"));
        }

        store
            .cancel_download(model_id)
            .map_err(|error| error.to_string())?;
        Ok(self.model_registry())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<Vec<ModelDescriptor>, String> {
        if model_id == self.settings.selected_model {
            return Err(format!("cannot delete active model {model_id}"));
        }

        let store = self.model_store();
        let registry = store
            .load_model_registry()
            .map_err(|error| error.to_string())?;
        if !registry.contains_key(model_id) {
            return Err(format!("unknown model {model_id}"));
        }
        let mut refreshed_models: Vec<ModelDescriptor> = registry.values().cloned().collect();
        store
            .refresh_statuses(&mut refreshed_models)
            .map_err(|error| error.to_string())?;
        let current_status = refreshed_models
            .iter()
            .find(|candidate| candidate.id == model_id)
            .map(|candidate| candidate.status)
            .unwrap_or(ModelStatus::Missing);
        if matches!(
            current_status,
            ModelStatus::Downloading | ModelStatus::Verifying | ModelStatus::Extracting
        ) {
            return Err(format!("model {model_id} download is active"));
        }

        store
            .delete_model(model_id)
            .map_err(|error| error.to_string())?;
        Ok(self.model_registry())
    }

    pub fn queue_snapshot(&self) -> QueueSnapshot {
        self.queue.snapshot()
    }

    pub fn recent_transcripts(&self, limit: usize) -> Vec<RecentTranscript> {
        recent_transcripts_from_save_root(&self.save_root_path(), limit)
    }

    pub fn upload_audio_file(
        &mut self,
        source_path: impl Into<std::path::PathBuf>,
        timestamp: DateTime<Local>,
    ) -> Result<UploadedAudio, String> {
        let source_path = source_path.into();
        let target = copy_uploaded_audio_file(&self.save_root_path(), &source_path, timestamp)
            .map_err(|error| error.to_string())?;
        Ok(UploadedAudio {
            audio_path: target.to_string_lossy().to_string(),
            original_filename: source_path
                .file_name()
                .and_then(|file_name| file_name.to_str())
                .unwrap_or_default()
                .to_string(),
            stored_at: timestamp.to_rfc3339(),
        })
    }

    pub fn start_capture_session(
        &mut self,
        sample_rate: u32,
        base_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<AppStatus, String> {
        let device_id = self.settings.selected_microphone.clone();
        let device_name = self.settings.selected_microphone_label.clone();
        self.start_capture_session_with_device(
            sample_rate,
            base_time,
            device_id,
            device_name,
            false,
        )
    }

    pub fn start_capture_session_with_device(
        &mut self,
        sample_rate: u32,
        base_time: chrono::DateTime<chrono::Utc>,
        device_id: impl Into<String>,
        device_name: impl Into<String>,
        used_fallback_device: bool,
    ) -> Result<AppStatus, String> {
        let device_id = device_id.into();
        let device_name = device_name.into();
        if let Some(capture) = self.capture.as_mut() {
            let events = capture.flush().map_err(|error| error.to_string())?;
            self.handle_capture_events(events);
        }
        self.level_monitor = LevelMonitor::default();
        self.active_microphone_label = Some(device_name.clone());
        self.microphone_warning = if used_fallback_device {
            Some(format!(
                "Pinned microphone {} is unavailable; using {device_name}",
                self.settings.selected_microphone_label
            ))
        } else {
            None
        };
        let using_default_device = device_id == "default" || used_fallback_device;
        self.mic_health
            .capture_started(Instant::now(), using_default_device);
        self.capture = Some(CaptureController::new(CaptureControllerConfig {
            save_root: self.save_root_path(),
            settings: self.settings.clone(),
            sample_rate,
            device_id,
            device_name,
            used_fallback_device,
            base_time,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
        }));
        Ok(self.app_status())
    }

    pub fn start_capture_session_for_test(&mut self, sample_rate: u32) -> Result<(), String> {
        self.start_capture_session(sample_rate, chrono::DateTime::<chrono::Utc>::UNIX_EPOCH)?;
        Ok(())
    }

    pub fn stop_capture_session(&mut self) -> Result<AppStatus, String> {
        if let Some(capture) = self.capture.as_mut() {
            let events = capture.flush().map_err(|error| error.to_string())?;
            self.handle_capture_events(events);
        }
        self.clear_capture_session_state();
        Ok(self.app_status())
    }

    pub fn capture_start_failed(&mut self, warning: impl Into<String>) -> AppStatus {
        self.clear_capture_session_state();
        self.microphone_warning = Some(warning.into());
        self.app_status()
    }

    /// Inspect microphone-input health and decide whether the watchdog should
    /// trigger a capture recovery. Returns `None` when no action is required.
    pub fn evaluate_microphone_health(&mut self) -> Option<MicHealthAction> {
        if self.capture.is_none() {
            return None;
        }
        if self.settings.pause_all || !self.settings.recording_enabled {
            return None;
        }
        match self.mic_health.tick(Instant::now()) {
            MicHealthVerdict::Action(action) => Some(action),
            _ => None,
        }
    }

    /// Pre-stage a device override for the next live-capture start. Consumed
    /// (cleared) by `take_microphone_recovery_override` at the next start.
    pub fn set_microphone_recovery_override(&mut self, device_id: impl Into<String>) {
        self.mic_recovery_override = Some(device_id.into());
    }

    /// Consume any pending recovery-device override; the next live-capture
    /// start should use the returned device id instead of `settings.selected_microphone`.
    pub fn take_microphone_recovery_override(&mut self) -> Option<String> {
        self.mic_recovery_override.take()
    }

    /// Surface a warning directly (used by the watchdog when recovery is
    /// exhausted) without tearing down the active capture session.
    pub fn set_microphone_warning(&mut self, warning: impl Into<String>) {
        self.microphone_warning = Some(warning.into());
    }

    /// Clear any standing microphone warning (used after a successful recovery
    /// re-establishes audio input).
    pub fn clear_microphone_warning(&mut self) {
        self.microphone_warning = None;
    }

    pub fn process_audio_frame(&mut self, frame: AudioFrame) -> Result<AppStatus, String> {
        let capture = self
            .capture
            .as_mut()
            .ok_or_else(|| "capture session is not running".to_string())?;
        let dbfs = dbfs_from_samples(&frame.samples);
        self.level_monitor.observe_samples(&frame.samples);
        self.mic_health.observe_frame(dbfs, Instant::now());
        let events = capture
            .process_samples_at(&frame.samples, frame.duration_ms, frame.captured_at)
            .map_err(|error| error.to_string())?;
        self.handle_capture_events(events);
        Ok(self.app_status())
    }

    pub fn process_audio_samples_for_test(
        &mut self,
        samples: &[f32],
        duration_ms: u64,
    ) -> Result<AppStatus, String> {
        let capture = self
            .capture
            .as_mut()
            .ok_or_else(|| "capture session is not running".to_string())?;
        self.level_monitor.observe_samples(samples);
        let events = capture
            .process_samples(samples, duration_ms)
            .map_err(|error| error.to_string())?;
        self.handle_capture_events(events);
        Ok(self.app_status())
    }

    pub fn enqueue_audio_file(
        &mut self,
        audio_path: impl Into<std::path::PathBuf>,
        model_id: Option<String>,
    ) -> QueueSnapshot {
        let audio_path = audio_path.into();
        if !is_importable_audio_path(&audio_path) {
            return self.queue.snapshot();
        }

        let model_id = model_id.unwrap_or_else(|| self.settings.selected_model.clone());
        self.queue.enqueue_file(audio_path, model_id);
        self.persist_queue();
        self.queue.snapshot()
    }

    pub fn enqueue_backlog(
        &mut self,
        save_root: impl AsRef<std::path::Path>,
    ) -> std::io::Result<QueueSnapshot> {
        let save_root = expand_user_path(save_root.as_ref().to_string_lossy());
        let scan = BacklogScan::scan(&save_root)?;
        self.queue
            .enqueue_backlog(scan, self.settings.selected_model.clone());
        self.persist_queue();
        Ok(self.queue.snapshot())
    }

    pub fn retry_job(&mut self, id: u64) -> Result<QueueSnapshot, String> {
        self.queue.retry(id)?;
        self.persist_queue();
        Ok(self.queue.snapshot())
    }

    pub fn skip_job(&mut self, id: u64) -> Result<QueueSnapshot, String> {
        self.queue.skip(id)?;
        self.persist_queue();
        Ok(self.queue.snapshot())
    }

    pub fn cancel_current_transcription(&mut self) -> Result<QueueSnapshot, String> {
        match self.queue.cancel_current("cancelled by user") {
            Ok(()) => self.persist_queue(),
            Err(error) if error == "no running job" => {}
            Err(error) => return Err(error),
        }
        Ok(self.queue.snapshot())
    }

    pub fn cancel_current_operation(&mut self) -> Result<(), String> {
        let store = self.model_store();
        if store
            .cancel_active_download()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(());
        }

        match self.queue.cancel_current("cancelled by user") {
            Ok(()) => {
                self.persist_queue();
                Ok(())
            }
            Err(error) if error == "no running job" => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn process_next_transcription(&mut self) -> Result<QueueSnapshot, String> {
        let transcriber = WhisperTranscriber::new(&self.settings.model_directory);
        self.process_next_transcription_with(transcriber)
    }

    pub fn process_next_transcription_with<T: Transcriber>(
        &mut self,
        transcriber: T,
    ) -> Result<QueueSnapshot, String> {
        if self.settings.pause_all || !self.settings.transcription_enabled {
            return Ok(self.queue.snapshot());
        }

        let selectable_model_ids = selectable_model_ids(&self.settings.model_directory);
        let Some(job) = self.queue.start_next_for_model_ids(&selectable_model_ids) else {
            return Ok(self.queue.snapshot());
        };
        let worker = TranscriptionWorker::with_options(
            transcriber,
            TranscriptionWorkerOptions {
                language: self.settings.transcription_language,
                suppress_low_confidence_transcripts: self
                    .settings
                    .suppress_low_confidence_transcripts,
            },
        );
        let outcome = worker
            .process_started_job(&job)
            .unwrap_or_else(|error| TranscriptionJobOutcome::failed(job.id, error.to_string()));
        self.finish_transcription_job(outcome)
    }

    pub fn process_pending_transcriptions_with<T: Clone + Transcriber>(
        &mut self,
        transcriber: T,
    ) -> Result<QueueSnapshot, String> {
        while let Some(started) = self.start_next_transcription_job() {
            let worker = TranscriptionWorker::with_options(
                transcriber.clone(),
                TranscriptionWorkerOptions {
                    language: started.language,
                    suppress_low_confidence_transcripts: started
                        .suppress_low_confidence_transcripts,
                },
            );
            let outcome = worker
                .process_started_job(&started.job)
                .unwrap_or_else(|error| {
                    TranscriptionJobOutcome::failed(started.job.id, error.to_string())
                });
            self.finish_transcription_job(outcome)?;
        }

        Ok(self.queue.snapshot())
    }

    pub fn should_process_transcriptions(&self) -> bool {
        let selectable_model_ids = selectable_model_ids(&self.settings.model_directory);
        !self.settings.pause_all
            && self.settings.transcription_enabled
            && self.queue.has_pending_for_model_ids(&selectable_model_ids)
    }

    pub fn start_next_transcription_job(&mut self) -> Option<StartedTranscriptionJob> {
        self.start_transcription_jobs_up_to(1).into_iter().next()
    }

    pub fn start_transcription_jobs_up_to(
        &mut self,
        max_running: usize,
    ) -> Vec<StartedTranscriptionJob> {
        if self.settings.pause_all || !self.settings.transcription_enabled {
            return Vec::new();
        }

        let selectable_model_ids = selectable_model_ids(&self.settings.model_directory);
        let model_directory = self.model_directory_path();
        let language = self.settings.transcription_language;
        let suppress_low_confidence_transcripts = self.settings.suppress_low_confidence_transcripts;
        let mut started_jobs = Vec::new();

        while let Some(job) = self
            .queue
            .start_next_for_model_ids_up_to(&selectable_model_ids, max_running)
        {
            started_jobs.push(StartedTranscriptionJob {
                job,
                model_directory: model_directory.clone(),
                language,
                suppress_low_confidence_transcripts,
            });
        }

        if !started_jobs.is_empty() {
            self.persist_queue();
        }
        started_jobs
    }

    pub fn finish_transcription_job(
        &mut self,
        outcome: TranscriptionJobOutcome,
    ) -> Result<QueueSnapshot, String> {
        apply_outcome(&mut self.queue, outcome).map_err(|error| error.to_string())?;
        self.persist_queue();
        Ok(self.queue.snapshot())
    }

    pub fn list_microphones(&self) -> Vec<MicrophoneDevice> {
        microphone_devices_from_input_devices(
            &self.settings.selected_microphone,
            list_input_devices(),
        )
    }

    pub fn app_status(&self) -> AppStatus {
        let queue = self.queue.snapshot();
        let mode = derive_mode(&self.settings);
        let is_recording = self
            .capture
            .as_ref()
            .map(|capture| capture.is_recording())
            .unwrap_or(false);
        let is_monitoring = self.capture.is_some();
        let has_error = self.microphone_warning.is_some() || queue.failed_count > 0;
        let tray_state = derive_tray_state(
            mode,
            queue.running_count > 0,
            is_monitoring,
            is_recording,
            has_error,
        );
        let runtime_warning = self.queue_blocked_warning(&queue);

        AppStatus {
            mode,
            tray_state,
            live_input_active: is_monitoring,
            active_model: self.settings.selected_model.clone(),
            active_microphone: self
                .active_microphone_label
                .clone()
                .unwrap_or_else(|| self.settings.selected_microphone_label.clone()),
            microphone_warning: self.microphone_warning.clone(),
            runtime_warning,
            threshold_dbfs: self.settings.threshold_dbfs,
            level: self.level_monitor.snapshot(),
            queue,
        }
    }

    /// Surface a runtime warning when the queue has pending jobs but the
    /// selected model is not installed, so the user understands why nothing
    /// is being transcribed.
    fn queue_blocked_warning(&self, queue: &QueueSnapshot) -> Option<String> {
        if queue.pending_count == 0 {
            return None;
        }
        if !self.settings.transcription_enabled || self.settings.pause_all {
            return None;
        }
        let installed = selectable_model_ids(&self.settings.model_directory);
        let stuck_models: HashSet<String> = queue
            .jobs
            .iter()
            .filter(|job| job.status == crate::queue::QueueJobStatus::Pending)
            .filter(|job| !installed.contains(&job.model_id))
            .map(|job| job.model_id.clone())
            .collect();
        if stuck_models.is_empty() {
            None
        } else {
            let mut models: Vec<String> = stuck_models.into_iter().collect();
            models.sort();
            Some(format!(
                "Transcription queue is paused: model {} is not installed. Open Models tab to download it.",
                models.join(", ")
            ))
        }
    }

    fn persist_settings(&self) {
        if let Some(persistence) = &self.persistence {
            let _ = persistence.save_settings(&self.settings);
        }
    }

    fn persist_queue(&self) {
        if let Some(persistence) = &self.persistence {
            let _ = persistence.save_queue(&self.queue);
        }
    }

    fn sync_capture_settings(&mut self) {
        let Some(capture) = self.capture.as_mut() else {
            return;
        };

        if let Ok(events) = capture.update_settings(self.settings.clone()) {
            self.handle_capture_events(events);
        }
        if self.settings.pause_all || !self.settings.recording_enabled {
            self.clear_capture_session_state();
        }
    }

    fn clear_capture_session_state(&mut self) {
        self.capture = None;
        self.active_microphone_label = None;
        self.level_monitor = LevelMonitor::default();
        self.mic_health.capture_stopped();
    }

    fn handle_capture_events(&mut self, events: Vec<CaptureControllerEvent>) {
        for event in events {
            match event {
                CaptureControllerEvent::ChunkStarted {
                    chunk_id,
                    started_at,
                } => {
                    eprintln!("[wakenote] capture: ChunkStarted chunk_id={chunk_id}");
                    self.emit_live_event(LiveTranscriptEvent::Started {
                        chunk_id,
                        started_at,
                        overlay_position: self.settings.floating_overlay_position,
                    });
                }
                CaptureControllerEvent::LiveSamplesReady {
                    chunk_id,
                    sample_rate,
                    samples,
                } => {
                    eprintln!(
                        "[wakenote] capture: LiveSamplesReady chunk_id={chunk_id} samples={} rate={sample_rate}",
                        samples.len()
                    );
                    self.emit_live_event(LiveTranscriptEvent::SamplesReady {
                        chunk_id,
                        model_id: self.settings.selected_model.clone(),
                        language: self.settings.transcription_language,
                        suppress_low_confidence_transcripts: self
                            .settings
                            .suppress_low_confidence_transcripts,
                        sample_rate,
                        samples,
                    });
                }
                CaptureControllerEvent::ChunkCompleted { chunk_id, chunk } => {
                    let model_id = self.transcription_model_for_completed_chunk(&chunk);
                    eprintln!(
                        "[wakenote] capture: ChunkCompleted chunk_id={chunk_id} path={} queue_model={:?}",
                        chunk.audio_path.display(),
                        model_id
                    );
                    self.remember_chunk_id(&chunk.audio_path, chunk_id);
                    if let Some(model_id) = model_id {
                        self.queue.enqueue_file(chunk.audio_path.clone(), model_id);
                    }
                    self.emit_live_event(LiveTranscriptEvent::Committed {
                        chunk_id,
                        audio_path: chunk.audio_path.clone(),
                        overlay_position: self.settings.floating_overlay_position,
                        will_transcribe: self.should_process_transcriptions(),
                    });
                }
            }
        }

        self.persist_queue();
    }

    fn emit_live_event(&mut self, event: LiveTranscriptEvent) {
        self.pending_live_events.push(event);
    }

    fn remember_chunk_id(&mut self, audio_path: &std::path::Path, chunk_id: u64) {
        let path = audio_path.to_path_buf();
        if self.chunk_id_index.insert(path.clone(), chunk_id).is_none() {
            self.chunk_id_history.push_back((path, chunk_id));
            while self.chunk_id_history.len() > LIVE_CHUNK_HISTORY_LIMIT {
                if let Some((evicted, _)) = self.chunk_id_history.pop_front() {
                    self.chunk_id_index.remove(&evicted);
                }
            }
        }
    }

    fn transcription_model_for_completed_chunk(&self, chunk: &RecordedChunk) -> Option<String> {
        let metadata = std::fs::read_to_string(&chunk.metadata_path)
            .ok()
            .and_then(|contents| serde_json::from_str::<ChunkMetadata>(&contents).ok());

        if let Some(metadata) = metadata {
            return (metadata.transcription_status == TranscriptionStatus::Queued)
                .then_some(metadata.model_id);
        }

        self.settings
            .transcription_enabled
            .then(|| self.settings.selected_model.clone())
    }
}

pub fn recent_transcripts_from_save_root(root: &Path, limit: usize) -> Vec<RecentTranscript> {
    if limit == 0 {
        return Vec::new();
    }

    let mut paths = Vec::new();
    collect_transcript_sidecar_paths(root, &mut paths);
    paths.sort_by(|left, right| {
        transcript_path_sort_key(right).cmp(&transcript_path_sort_key(left))
    });
    paths.truncate(limit);

    paths
        .into_iter()
        .filter_map(|path| recent_transcript_from_sidecar(&path))
        .collect()
}

fn collect_transcript_sidecar_paths(root: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_transcript_sidecar_paths(&path, paths);
            continue;
        }

        if !file_type.is_file() {
            continue;
        }
        if !is_transcript_sidecar(&path) {
            continue;
        }

        paths.push(path);
    }
}

fn recent_transcript_from_sidecar(path: &Path) -> Option<RecentTranscript> {
    let text = fs::read_to_string(path).ok()?.trim().to_string();
    if text.is_empty() {
        return None;
    }

    Some(RecentTranscript {
        transcript_path: path.to_string_lossy().to_string(),
        audio_path: audio_path_for_transcript(path)
            .map(|audio_path| audio_path.to_string_lossy().to_string()),
        recorded_at: recorded_at_for_transcript(path),
        text,
    })
}

fn transcript_path_sort_key(path: &Path) -> String {
    recorded_at_from_path(path).unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn audio_path_for_transcript(path: &Path) -> Option<PathBuf> {
    ["m4a", "wav"]
        .into_iter()
        .map(|extension| path.with_extension(extension))
        .find(|candidate| candidate.exists())
}

fn recorded_at_for_transcript(path: &Path) -> String {
    path.with_extension("json")
        .try_exists()
        .ok()
        .filter(|exists| *exists)
        .and_then(|_| fs::read(path.with_extension("json")).ok())
        .and_then(|bytes| serde_json::from_slice::<ChunkMetadata>(&bytes).ok())
        .map(|metadata| metadata.started_at.to_rfc3339())
        .or_else(|| recorded_at_from_path(path))
        .unwrap_or_else(|| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .map(DateTime::<Utc>::from)
                .map(|timestamp| timestamp.to_rfc3339())
                .unwrap_or_default()
        })
}

fn recorded_at_from_path(path: &Path) -> Option<String> {
    let date_part = path.parent()?.file_name()?.to_str()?;
    if date_part.len() != 8
        || !date_part
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return None;
    }

    let stem = path.file_stem()?.to_str()?;
    let time_part = stem.get(0..6)?;
    if !time_part
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        return None;
    }

    let date = NaiveDate::parse_from_str(date_part, "%Y%m%d").ok()?;
    let time = NaiveTime::parse_from_str(time_part, "%H%M%S").ok()?;
    let local = date.and_time(time);
    match Local.from_local_datetime(&local) {
        chrono::LocalResult::Single(timestamp) => Some(timestamp.to_rfc3339()),
        chrono::LocalResult::Ambiguous(timestamp, _) => Some(timestamp.to_rfc3339()),
        chrono::LocalResult::None => {
            Some(DateTime::<Utc>::from_naive_utc_and_offset(local, Utc).to_rfc3339())
        }
    }
}

fn is_transcript_sidecar(path: &Path) -> bool {
    if path.extension().and_then(|extension| extension.to_str()) != Some("txt") {
        return false;
    }

    !path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name.ends_with(".error.txt"))
}

pub fn microphone_devices_from_input_devices(
    selected_microphone: &str,
    devices: Vec<InputDevice>,
) -> Vec<MicrophoneDevice> {
    devices
        .into_iter()
        .map(|device| MicrophoneDevice {
            fallback: device.id == "default" && selected_microphone != "default",
            id: device.id,
            label: device.label,
            available: device.available,
        })
        .collect()
}

pub fn model_registry_snapshot(
    model_directory: impl AsRef<std::path::Path>,
) -> Vec<ModelDescriptor> {
    let store = ModelStore::new(model_directory);
    let download_state = store.load_download_state().unwrap_or_default();
    let mut models: Vec<ModelDescriptor> = store
        .load_model_registry()
        .unwrap_or_else(|_| default_model_registry())
        .into_values()
        .map(|mut model| {
            model.download_progress = None;
            model.download_error = None;

            if store.model_path(&model.id).exists() {
                model.status = ModelStatus::Ready;
                model.download_progress = Some(100);
                return model;
            }

            if let Some(record) = download_state.downloads.get(&model.id) {
                model.status = record.status;
                model.download_progress = record.download_progress_percent();
                model.download_error = record.error.clone();
            } else {
                model.status = ModelStatus::Missing;
            }

            model
        })
        .collect();
    models.sort_by(|left, right| left.id.cmp(&right.id));
    models
}

pub fn derive_mode(settings: &AppSettings) -> AppMode {
    if settings.pause_all || (!settings.recording_enabled && !settings.transcription_enabled) {
        return AppMode::Paused;
    }

    match (settings.recording_enabled, settings.transcription_enabled) {
        (true, true) => AppMode::RecordingAndTranscription,
        (true, false) => AppMode::RecordingOnly,
        (false, true) => AppMode::TranscriptionOnly,
        (false, false) => AppMode::Paused,
    }
}

fn derive_tray_state(
    mode: AppMode,
    transcribing: bool,
    monitoring: bool,
    recording: bool,
    has_error: bool,
) -> TrayState {
    if recording {
        return TrayState::Recording;
    }

    if transcribing {
        return TrayState::Transcribing;
    }

    if has_error {
        return TrayState::Error;
    }

    match mode {
        AppMode::RecordingAndTranscription | AppMode::RecordingOnly if monitoring => {
            TrayState::Listening
        }
        AppMode::RecordingAndTranscription | AppMode::RecordingOnly => TrayState::Idle,
        AppMode::TranscriptionOnly => TrayState::Idle,
        AppMode::Paused => TrayState::Paused,
    }
}

fn model_is_selectable(model_id: &str, model_directory: &str) -> bool {
    selectable_model_ids(model_directory).contains(model_id)
}

fn selectable_model_ids(model_directory: &str) -> HashSet<String> {
    let store = ModelStore::new(expand_user_path(model_directory));
    let models = store
        .load_model_registry()
        .unwrap_or_else(|_| default_model_registry())
        .into_values();
    let download_state = store.load_download_state().unwrap_or_default();

    models
        .filter(|model| {
            store.model_path(&model.id).exists()
                && !download_state
                    .downloads
                    .get(&model.id)
                    .is_some_and(download_record_blocks_model_selection)
        })
        .map(|model| model.id)
        .collect()
}

fn download_record_blocks_model_selection(record: &crate::models::ModelDownloadRecord) -> bool {
    matches!(
        record.status,
        ModelStatus::Downloading
            | ModelStatus::Verifying
            | ModelStatus::Extracting
            | ModelStatus::Error
    )
}

pub fn with_runtime_warning(status: AppStatus, dropped_frames: u64) -> AppStatus {
    with_live_runtime_warning(status, dropped_frames, None)
}

pub fn with_live_runtime_warning(
    mut status: AppStatus,
    dropped_frames: u64,
    stream_error: Option<String>,
) -> AppStatus {
    if let Some(stream_error) = stream_error.filter(|error| !error.trim().is_empty()) {
        status.tray_state = TrayState::Error;
        status.runtime_warning = Some(format!("Live input stream error: {stream_error}"));
        return status;
    }

    if dropped_frames > 0 {
        status.runtime_warning = Some(format!(
            "Live input dropped {dropped_frames} stale audio frames while processing was busy"
        ));
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_registry(model_directory: &std::path::Path, model_id: &str) {
        std::fs::create_dir_all(model_directory).expect("model dir");
        std::fs::write(
            model_directory.join("model-registry.json"),
            format!(
                r#"[
                  {{
                    "id": "{model_id}",
                    "display_name": "Local Test Model",
                    "engine": "whisper.cpp",
                    "provider_runtime": "whisper-rs",
                    "download_url": null,
                    "checksum_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "size_mb": 1,
                    "languages": ["en"],
                    "speed_score": 1,
                    "accuracy_score": 1,
                    "offline": true
                  }}
                ]"#
            ),
        )
        .expect("registry");
    }

    #[test]
    fn selectable_model_ids_uses_file_existence_without_hashing_model_contents() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_test_registry(tmp.path(), "local-ready");
        std::fs::write(
            tmp.path().join("local-ready.bin"),
            b"checksum is intentionally wrong",
        )
        .expect("model file");

        let models = selectable_model_ids(&tmp.path().to_string_lossy());

        assert!(models.contains("local-ready"));
    }

    #[test]
    fn selectable_model_ids_excludes_models_with_active_download_records() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_test_registry(tmp.path(), "local-downloading");
        std::fs::write(tmp.path().join("local-downloading.bin"), b"existing model")
            .expect("model file");
        ModelStore::new(tmp.path())
            .record_download_progress("local-downloading", 1, Some(2))
            .expect("download state");

        let models = selectable_model_ids(&tmp.path().to_string_lossy());

        assert!(!models.contains("local-downloading"));
    }

    #[test]
    fn tray_state_prefers_active_recording_over_running_transcription() {
        assert_eq!(
            derive_tray_state(AppMode::RecordingAndTranscription, true, true, true, false),
            TrayState::Recording
        );
    }
}
