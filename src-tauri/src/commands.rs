use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
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
use crate::queue::{
    BacklogScan, COMPLETED_JOB_HISTORY_LIMIT, QueueSnapshot, TranscriptionQueue,
    is_importable_audio_path,
};
use crate::recorder::{ChunkMetadata, ChunkSource, RecordedChunk, TranscriptionStatus};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayIconImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayRuntimePresentation {
    pub icon: TrayPresentation,
    pub visible: bool,
}

const TRAY_ICON_NORMAL_RGBA: [u8; 4] = [0, 0, 0, 255];
const TRAY_ICON_RECORDING_RGBA: [u8; 4] = [22, 163, 74, 255];
const TRAY_ICON_TRANSCRIBING_RGBA: [u8; 4] = [217, 119, 6, 255];
const TRAY_ICON_DISCONNECTED_RGBA: [u8; 4] = [220, 38, 38, 255];
const TRAY_ICON_IMAGE_SIZE: u32 = 64;
const TRAY_ICON_DOT_DIAMETER: u32 = TRAY_ICON_IMAGE_SIZE / 2;

pub fn tray_presentation_for_state(state: TrayState) -> TrayPresentation {
    match state {
        TrayState::Idle => TrayPresentation {
            rgba: TRAY_ICON_NORMAL_RGBA,
            tooltip: "WakeNote: Idle",
        },
        TrayState::Listening => TrayPresentation {
            rgba: TRAY_ICON_NORMAL_RGBA,
            tooltip: "WakeNote: Listening",
        },
        TrayState::Recording => TrayPresentation {
            rgba: TRAY_ICON_RECORDING_RGBA,
            tooltip: "WakeNote: Recording",
        },
        TrayState::Transcribing => TrayPresentation {
            rgba: TRAY_ICON_TRANSCRIBING_RGBA,
            tooltip: "WakeNote: Transcribing",
        },
        TrayState::Paused => TrayPresentation {
            rgba: TRAY_ICON_NORMAL_RGBA,
            tooltip: "WakeNote: Paused",
        },
        TrayState::Error => TrayPresentation {
            rgba: TRAY_ICON_NORMAL_RGBA,
            tooltip: "WakeNote: Error",
        },
    }
}

pub fn tray_icon_image_for_presentation(presentation: TrayPresentation) -> TrayIconImage {
    let width = TRAY_ICON_IMAGE_SIZE;
    let height = TRAY_ICON_IMAGE_SIZE;
    let radius = TRAY_ICON_DOT_DIAMETER as f32 / 2.0;
    let center = TRAY_ICON_IMAGE_SIZE as f32 / 2.0;
    let mut rgba = vec![0; (width * height * 4) as usize];

    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 + 0.5 - center;
            let dy = y as f32 + 0.5 - center;
            let distance = dx.mul_add(dx, dy * dy).sqrt();
            let alpha = if distance <= radius - 0.5 {
                255
            } else if distance < radius + 0.5 {
                ((radius + 0.5 - distance) * 255.0).round() as u8
            } else {
                0
            };
            if alpha > 0 {
                let offset = ((y * width + x) * 4) as usize;
                rgba[offset..offset + 3].copy_from_slice(&presentation.rgba[..3]);
                rgba[offset + 3] = alpha;
            }
        }
    }

    TrayIconImage {
        rgba,
        width,
        height,
    }
}

pub fn tray_runtime_presentation(
    settings: &AppSettings,
    status: &AppStatus,
) -> TrayRuntimePresentation {
    let mut icon = tray_presentation_for_state(status.tray_state);
    if tray_status_has_microphone_connection_failure(status) {
        icon.rgba = TRAY_ICON_DISCONNECTED_RGBA;
        icon.tooltip = "WakeNote: Microphone disconnected";
    }

    TrayRuntimePresentation {
        icon,
        visible: settings.show_tray_icon,
    }
}

fn tray_status_has_microphone_connection_failure(status: &AppStatus) -> bool {
    status.microphone_warning.is_some()
        || status
            .runtime_warning
            .as_deref()
            .map(|warning| warning.starts_with("Live input stream error:"))
            .unwrap_or(false)
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

/// Decide whether the device we actually opened diverges from the user's
/// pinned selection enough to warrant a "fallback in use" warning. The
/// `cpal` stable id embeds the host enumeration index, which is NOT stable
/// across sleep/wake or USB re-enumeration on macOS — the same physical mic
/// can end up with a different id while keeping its label. The label is the
/// durable identity, so a label match is enough evidence that we are on
/// the user's intended device and no warning should fire.
pub fn pinned_device_mismatch(
    pinned_id: &str,
    pinned_label: &str,
    resolved_id: &str,
    resolved_label: &str,
    resolver_fell_back: bool,
) -> bool {
    if resolver_fell_back {
        return true;
    }
    if pinned_id == resolved_id {
        return false;
    }
    if !pinned_label.is_empty() && pinned_label == resolved_label {
        return false;
    }
    true
}

/// Rewrite any settings entry whose `id == requested_id` to point at
/// `resolved_id` when the label still matches. Closes the drift where
/// `cpal` re-enumeration assigns the same physical mic a different stable
/// id, leaving stale ids in `selected_microphone` and the priority list.
/// Pure — does not persist; the caller is responsible for `persist_settings`.
pub fn reconcile_device_id_in_settings(
    settings: &mut AppSettings,
    requested_id: &str,
    resolved_id: &str,
    resolved_label: &str,
) -> bool {
    if requested_id == resolved_id || resolved_label.is_empty() {
        return false;
    }
    let mut changed = false;
    if settings.selected_microphone == requested_id
        && settings.selected_microphone_label == resolved_label
    {
        settings.selected_microphone = resolved_id.to_string();
        changed = true;
    }
    for entry in settings.microphone_priority.iter_mut() {
        if entry.id == requested_id && entry.label == resolved_label {
            entry.id = resolved_id.to_string();
            changed = true;
        }
    }
    changed
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SilenceWarning {
    /// User-visible label of the device that has been silent.
    pub device_label: String,
    /// How long the input has been digitally silent, in whole seconds.
    pub seconds: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppStatus {
    pub mode: AppMode,
    pub tray_state: TrayState,
    pub live_input_active: bool,
    pub active_model: String,
    pub active_microphone: String,
    pub microphone_warning: Option<String>,
    pub silence_warning: Option<SilenceWarning>,
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
pub struct TranscriptDay {
    pub day: String,
    pub count: usize,
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

/// One-shot device override staged by the recovery watchdog. Carries the
/// stable device id plus the persisted label, so the resolver can still
/// reattach to the same physical mic when its enumeration index has moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicRecoveryOverride {
    pub device_id: String,
    pub label_hint: Option<String>,
}

pub struct AppBackend {
    settings: AppSettings,
    queue: TranscriptionQueue,
    capture: Option<CaptureController>,
    /// Parallel capture session for system-audio frames. Mirrors `capture`
    /// but has no level/health monitoring and tags chunks `source = System`.
    system_capture: Option<CaptureController>,
    level_monitor: LevelMonitor,
    active_microphone_label: Option<String>,
    microphone_warning: Option<String>,
    silence_warning: Option<SilenceWarning>,
    persistence: Option<AppPersistence>,
    live_event_handler: Option<LiveEventHandler>,
    pending_live_events: Vec<LiveTranscriptEvent>,
    chunk_id_history: VecDeque<(PathBuf, u64)>,
    chunk_id_index: HashMap<PathBuf, u64>,
    mic_health: MicHealthMonitor,
    /// One-shot device override used by the recovery watchdog to force a
    /// restart onto a specific device (e.g. system default) on the next
    /// capture start. Consumed by `take_microphone_recovery_override`.
    mic_recovery_override: Option<MicRecoveryOverride>,
}

impl std::fmt::Debug for AppBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppBackend")
            .field("settings", &self.settings)
            .field("queue", &self.queue)
            .field("capture", &self.capture)
            .field("system_capture", &self.system_capture)
            .field("level_monitor", &self.level_monitor)
            .field("active_microphone_label", &self.active_microphone_label)
            .field("microphone_warning", &self.microphone_warning)
            .field("silence_warning", &self.silence_warning)
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
            system_capture: None,
            level_monitor: LevelMonitor::default(),
            active_microphone_label: None,
            microphone_warning: None,
            silence_warning: None,
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
            system_capture: None,
            level_monitor: LevelMonitor::default(),
            active_microphone_label: None,
            microphone_warning: None,
            silence_warning: None,
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
        // Runtime diagnostic: every capture start logs the resolved device id,
        // its label, and whether a fallback warning is being surfaced. Lets
        // a `log show --predicate 'process == "wakenote"'` reader confirm
        // the false-positive 'Pinned X unavailable; using X' bug stays dead.
        eprintln!(
            "[mic-start] device_id={device_id} label={device_name} fallback={used_fallback_device} warning={:?}",
            self.microphone_warning
        );
        let priority_ids: Vec<String> = self
            .settings
            .microphone_priority
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        let active_index = priority_ids
            .iter()
            .position(|id| id == &device_id)
            .unwrap_or(priority_ids.len());
        self.mic_health
            .capture_started(Instant::now(), priority_ids, active_index);
        self.capture = Some(CaptureController::new(CaptureControllerConfig {
            save_root: self.save_root_path(),
            settings: self.settings.clone(),
            sample_rate,
            device_id,
            device_name,
            used_fallback_device,
            base_time,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            source: ChunkSource::Microphone,
            source_label: None,
        }));
        Ok(self.app_status())
    }

    pub fn start_capture_session_for_test(&mut self, sample_rate: u32) -> Result<(), String> {
        self.start_capture_session(sample_rate, chrono::DateTime::<chrono::Utc>::UNIX_EPOCH)?;
        Ok(())
    }

    /// Replace the mic-health monitor with one using a custom config.
    /// Call this before `start_capture_session_for_test` so that
    /// `capture_started` is invoked on the new monitor.
    pub fn override_mic_health_config_for_test(&mut self, config: crate::audio::MicHealthConfig) {
        self.mic_health = MicHealthMonitor::with_config(config);
    }

    /// Inject a single audio frame directly into the health monitor.
    pub fn observe_audio_for_test(&mut self, dbfs: f32, at: Instant) {
        self.mic_health.observe_frame(dbfs, at);
    }

    /// Test-only: apply a `SettingsPatch` without going through the Tauri
    /// command boundary. Mirrors `update_settings` but lets integration tests
    /// drive backend state without a Tauri runtime.
    pub fn apply_settings_patch_for_test(&mut self, patch: crate::settings::SettingsPatch) {
        self.update_settings(patch);
    }

    pub fn stop_capture_session(&mut self) -> Result<AppStatus, String> {
        if let Some(capture) = self.capture.as_mut() {
            let events = capture.flush().map_err(|error| error.to_string())?;
            self.handle_capture_events(events);
        }
        self.clear_capture_session_state();
        Ok(self.app_status())
    }

    /// Mark the most recent live-capture start as failed. The capture
    /// controller is torn down, but mic_health stays alive in
    /// `AwaitingRestart` so the watchdog keeps cycling through the priority
    /// list with backoff until a start succeeds.
    pub fn capture_start_failed(&mut self, warning: impl Into<String>) -> AppStatus {
        self.capture = None;
        self.active_microphone_label = None;
        self.level_monitor = LevelMonitor::default();
        self.silence_warning = None;

        let warning = warning.into();
        let priority_ids: Vec<String> = self
            .settings
            .microphone_priority
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        self.mic_health.refresh_priority(priority_ids);
        self.mic_health
            .mark_start_failed(Instant::now(), warning.clone());

        self.microphone_warning = Some(warning);
        self.app_status()
    }

    /// Inspect microphone-input health and decide whether the watchdog should
    /// trigger a capture recovery. Returns `None` when no action is required.
    /// Side effect: updates `self.silence_warning` based on the verdict so
    /// `app_status()` exposes the warning to the frontend.
    pub fn evaluate_microphone_health(&mut self) -> Option<MicHealthAction> {
        self.evaluate_microphone_health_at(Instant::now())
    }

    pub fn evaluate_microphone_health_at(&mut self, now: Instant) -> Option<MicHealthAction> {
        if self.settings.pause_all || !self.settings.recording_enabled {
            self.silence_warning = None;
            return None;
        }
        // Allow ticking when no capture is running so the watchdog can keep
        // re-attempting starts after a failure (`AwaitingRestart`). When
        // capture is fully stopped (`Idle`) mic_health returns Healthy and
        // we exit cleanly.
        if self.capture.is_none() && !self.mic_health.is_awaiting_restart() {
            self.silence_warning = None;
            return None;
        }
        let verdict = self.mic_health.tick(now);
        match verdict {
            MicHealthVerdict::Action(action) => {
                self.silence_warning = None;
                Some(action)
            }
            MicHealthVerdict::SilenceWarning { since } => {
                let device_label = self
                    .active_microphone_label
                    .clone()
                    .unwrap_or_else(|| self.settings.selected_microphone_label.clone());
                self.silence_warning = Some(SilenceWarning {
                    device_label,
                    seconds: since.as_secs() as u32,
                });
                None
            }
            MicHealthVerdict::Healthy
            | MicHealthVerdict::AwaitingFirstFrame
            | MicHealthVerdict::InCooldown => {
                self.silence_warning = None;
                None
            }
        }
    }

    /// True iff recording is desired (recording_enabled && !pause_all). The
    /// watchdog uses this to decide whether to drive an idle-but-pending
    /// recovery loop forward, even when no live stream is currently running.
    pub fn live_capture_should_run(&self) -> bool {
        self.settings.recording_enabled && !self.settings.pause_all
    }

    /// True iff the watchdog is in the middle of recovering from a failed
    /// start (no live capture, but periodic retries pending).
    pub fn mic_recovery_pending(&self) -> bool {
        self.capture.is_none() && self.mic_health.is_awaiting_restart()
    }

    /// Number of consecutive watchdog escalations since the last non-silent
    /// frame. Used by the runtime watchdog to surface a UI warning when
    /// recovery is fighting (and likely losing).
    pub fn mic_escalations_since_first_frame(&self) -> u32 {
        self.mic_health.escalations_since_first_frame()
    }

    /// Forward an out-of-band stall signal (cpal `StreamError`, OS device-change)
    /// to the health monitor so the next watchdog tick triggers a recovery
    /// action immediately instead of waiting for heartbeat/silence thresholds.
    pub fn notify_stream_error(&mut self, reason: impl Into<String>) {
        if self.capture.is_none() {
            return;
        }
        self.mic_health.request_recovery(reason);
    }

    /// Pre-stage a device override for the next live-capture start. Consumed
    /// (cleared) by `take_microphone_recovery_override` at the next start.
    /// The label is looked up from the user's priority list so the resolver
    /// can fall back to label-matching when the cpal index has moved.
    pub fn set_microphone_recovery_override(&mut self, device_id: impl Into<String>) {
        let device_id = device_id.into();
        let label_hint = self
            .settings
            .microphone_priority
            .iter()
            .find(|entry| entry.id == device_id)
            .map(|entry| entry.label.clone())
            .or_else(|| {
                if self.settings.selected_microphone == device_id {
                    Some(self.settings.selected_microphone_label.clone())
                } else {
                    None
                }
            });
        self.mic_recovery_override = Some(MicRecoveryOverride {
            device_id,
            label_hint,
        });
    }

    /// Consume any pending recovery-device override; the next live-capture
    /// start should use the returned device id instead of `settings.selected_microphone`.
    pub fn take_microphone_recovery_override(&mut self) -> Option<MicRecoveryOverride> {
        self.mic_recovery_override.take()
    }

    /// Update settings to point at the actual stable id we just resolved to,
    /// when the label proves we're on the same physical device the user
    /// pinned. Silently reconciles drift after cpal re-enumeration so the
    /// next start has the correct id from the get-go.
    pub fn reconcile_resolved_device_id(
        &mut self,
        requested_id: &str,
        resolved_id: &str,
        resolved_label: &str,
    ) {
        if reconcile_device_id_in_settings(
            &mut self.settings,
            requested_id,
            resolved_id,
            resolved_label,
        ) {
            self.persist_settings();
        }
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

    /// Open a parallel capture session for system-audio frames. Unlike the
    /// microphone path this has no level meter, no health watchdog and no
    /// device resolution: the caller (system-audio input) already owns those.
    /// Chunks it produces are tagged `source = System` and slugged with
    /// `source_id` so they never collide with mic chunks from the same second.
    pub fn start_system_capture_session(
        &mut self,
        sample_rate: u32,
        base_time: chrono::DateTime<chrono::Utc>,
        app_name: String,
        source_id: String,
    ) -> Result<AppStatus, String> {
        if let Some(capture) = self.system_capture.as_mut() {
            let events = capture.flush().map_err(|error| error.to_string())?;
            self.handle_system_capture_events(events);
        }
        self.system_capture = Some(CaptureController::new(CaptureControllerConfig {
            save_root: self.save_root_path(),
            settings: self.settings.clone(),
            sample_rate,
            device_id: source_id.clone(),
            device_name: app_name,
            used_fallback_device: false,
            base_time,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            source: ChunkSource::System,
            source_label: Some(source_id),
        }));
        Ok(self.app_status())
    }

    /// Feed one system-audio frame into the parallel capture session. Mirrors
    /// `process_audio_frame` but routes completed chunks through the same
    /// transcription queue without touching the mic level/health monitors or
    /// emitting mic-specific live-capture events.
    pub fn process_system_audio_frame(&mut self, frame: AudioFrame) -> Result<AppStatus, String> {
        let capture = self
            .system_capture
            .as_mut()
            .ok_or_else(|| "system capture session is not running".to_string())?;
        let events = capture
            .process_samples_at(&frame.samples, frame.duration_ms, frame.captured_at)
            .map_err(|error| error.to_string())?;
        self.handle_system_capture_events(events);
        Ok(self.app_status())
    }

    /// Flush the active system chunk and tear down the system capture session.
    /// Mirrors `stop_capture_session`'s flush-then-clear so an in-progress
    /// system recording is finalized and enqueued before the session closes.
    pub fn stop_system_capture_session(&mut self) -> Result<AppStatus, String> {
        if let Some(capture) = self.system_capture.as_mut() {
            let events = capture.flush().map_err(|error| error.to_string())?;
            self.handle_system_capture_events(events);
        }
        self.system_capture = None;
        Ok(self.app_status())
    }

    /// Enqueue completed system chunks into the shared transcription queue,
    /// honoring the same transcription-enabled + single-insert dedup rules as
    /// the mic path. System chunks do not emit live-capture/overlay events for
    /// v1 (that plumbing is mic-specific); they are recorded and enqueued only.
    fn handle_system_capture_events(&mut self, events: Vec<CaptureControllerEvent>) {
        let mut queue_changed = false;
        for event in events {
            if let CaptureControllerEvent::ChunkCompleted { chunk_id, chunk } = event {
                let model_id = self.transcription_model_for_completed_chunk(&chunk);
                eprintln!(
                    "[wakenote] system-capture: ChunkCompleted chunk_id={chunk_id} path={} queue_model={:?}",
                    chunk.audio_path.display(),
                    model_id
                );
                self.remember_chunk_id(&chunk.audio_path, chunk_id);
                if let Some(model_id) = model_id {
                    let (_, inserted) = self
                        .queue
                        .enqueue_file_if_new(chunk.audio_path.clone(), model_id);
                    queue_changed |= inserted;
                }
            }
        }

        if queue_changed {
            self.persist_queue();
        }
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
        let (_, inserted) = self.queue.enqueue_file_if_new(audio_path, model_id);
        if inserted {
            self.persist_queue();
        }
        self.queue.snapshot()
    }

    pub fn enqueue_backlog(
        &mut self,
        save_root: impl AsRef<std::path::Path>,
    ) -> std::io::Result<QueueSnapshot> {
        let save_root = expand_user_path(save_root.as_ref().to_string_lossy());
        let scan = BacklogScan::scan(&save_root)?;
        let enqueued = self
            .queue
            .enqueue_backlog(scan, self.settings.selected_model.clone());
        if !enqueued.is_empty() {
            self.persist_queue();
        }
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
            silence_warning: self.silence_warning.clone(),
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

    fn persist_queue(&mut self) {
        self.queue
            .prune_completed_history(COMPLETED_JOB_HISTORY_LIMIT);
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

    /// Propagate the current settings to an active system-audio capture session
    /// so a settings change mid-capture (e.g. threshold/chunk timing) takes
    /// effect without restarting the stream. Mirrors `sync_capture_settings`'s
    /// controller update for the mic path; tearing the session down on a
    /// disabled/paused state is the caller's job (it also owns the stream
    /// handle), so this is a no-op when no system capture is running.
    pub fn sync_system_capture_settings(&mut self) {
        let Some(capture) = self.system_capture.as_mut() else {
            return;
        };
        if let Ok(events) = capture.update_settings(self.settings.clone()) {
            self.handle_system_capture_events(events);
        }
    }

    /// Whether a system-audio capture session is currently open. Used by the
    /// settings path to decide if it must tear down or sync the session, and by
    /// tests to assert the disable/pause teardown behavior.
    pub fn is_system_capturing(&self) -> bool {
        self.system_capture.is_some()
    }

    fn clear_capture_session_state(&mut self) {
        self.capture = None;
        self.active_microphone_label = None;
        self.level_monitor = LevelMonitor::default();
        self.mic_health.capture_stopped();
        self.silence_warning = None;
    }

    fn handle_capture_events(&mut self, events: Vec<CaptureControllerEvent>) {
        let mut queue_changed = false;
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
                        let (_, inserted) =
                            self.queue.enqueue_file_if_new(chunk.audio_path.clone(), model_id);
                        queue_changed |= inserted;
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

        if queue_changed {
            self.persist_queue();
        }
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

/// Lists the `YYYY-MM-DD` days that contain at least one non-empty transcript
/// sidecar, with a per-day sidecar count. Scans both top-level `YYYYMMDD`
/// folders and `uploaded/YYYYMMDD`. Reads `.txt` content (not just metadata) so
/// the count matches what the day view actually renders: empty/suppressed
/// sidecars are excluded, exactly as `recent_transcript_from_sidecar` does.
pub fn transcript_days_from_save_root(root: &Path) -> Vec<TranscriptDay> {
    // Counts are size-based (see `sidecar_has_content`): no file content is read
    // here, so this never triggers an iCloud download and works on evicted
    // (dataless) days. `accumulate_day_counts` never inserts zero-count entries,
    // so every entry is a day with at least one sidecar. BTreeMap keeps days sorted.
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    accumulate_day_counts(root, &mut counts);
    accumulate_day_counts(&root.join("uploaded"), &mut counts);

    counts
        .into_iter()
        .map(|(day, count)| TranscriptDay { day, count })
        .collect()
}

fn accumulate_day_counts(parent: &Path, counts: &mut BTreeMap<String, usize>) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let Some(day) = dashed_day_from_compact(name.to_str().unwrap_or_default()) else {
            continue;
        };
        let count = count_transcript_sidecars(&entry.path());
        // Skip empty days so the map only ever holds days with sidecars.
        if count > 0 {
            *counts.entry(day).or_insert(0) += count;
        }
    }
}

/// Counts transcript sidecars in a single day directory using file SIZE only
/// (no content read). A sidecar counts when it is a non-`.error.txt` `.txt`
/// larger than one byte. Empty/suppressed transcripts are written as a lone
/// `"\n"` (one byte) by `write_text_sidecar`, so `len > 1` excludes them,
/// keeping the count aligned with what the day view (which trims) renders.
///
/// Size comes from `stat`, which returns the logical size of an iCloud
/// "dataless" (evicted) file WITHOUT downloading it. Reading content here would
/// instead fail under the dataless guard — so evicted days would wrongly count
/// as empty — or force a download of the entire archive.
fn count_transcript_sidecars(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().map(|kind| kind.is_file()).unwrap_or(false))
        .map(|entry| entry.path())
        .filter(|path| is_transcript_sidecar(path))
        .filter(|path| sidecar_has_content(path))
        .count()
}

/// Whether a sidecar holds real content, judged from size alone so it works on
/// iCloud dataless files without downloading them. One byte or less is the
/// empty/suppressed `"\n"` stub and counts as contentless.
fn sidecar_has_content(path: &Path) -> bool {
    fs::metadata(path).map(|meta| meta.len() > 1).unwrap_or(false)
}

/// "20260510" -> Some("2026-05-10"); anything that is not 8 ASCII digits -> None.
fn dashed_day_from_compact(name: &str) -> Option<String> {
    if name.len() == 8 && name.bytes().all(|byte| byte.is_ascii_digit()) {
        Some(format!("{}-{}-{}", &name[0..4], &name[4..6], &name[6..8]))
    } else {
        None
    }
}

/// "2026-05-10" -> Some("20260510"); malformed input -> None.
fn compact_day_from_dashed(day: &str) -> Option<String> {
    let bytes = day.as_bytes();
    let well_formed = day.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && day[0..4].bytes().all(|byte| byte.is_ascii_digit())
        && day[5..7].bytes().all(|byte| byte.is_ascii_digit())
        && day[8..10].bytes().all(|byte| byte.is_ascii_digit());
    well_formed.then(|| format!("{}{}{}", &day[0..4], &day[5..7], &day[8..10]))
}

/// Materializes every non-empty transcript sidecar for a single `YYYY-MM-DD`
/// day, reading only that day's `YYYYMMDD` folder (and its `uploaded` twin).
/// Entries are returned in ascending (oldest-first) order.
///
/// When `download` is false (on-navigation load) the dataless guard is held so
/// iCloud-evicted sidecars are skipped instead of downloaded. When `download`
/// is true (an explicit user reload) the guard is dropped so the requested
/// day's evicted sidecars are materialized (fetched) and returned.
pub fn transcripts_for_day_from_save_root(
    root: &Path,
    day: &str,
    download: bool,
) -> Vec<RecentTranscript> {
    let Some(compact) = compact_day_from_dashed(day) else {
        return Vec::new();
    };

    let _dataless_guard = if download {
        None
    } else {
        Some(DatalessMaterializationGuard::disabled())
    };
    let day_dir = root.join(&compact);
    let uploaded_dir = root.join("uploaded").join(&compact);
    let mut paths = Vec::new();
    collect_day_sidecar_paths(&day_dir, &mut paths);
    collect_day_sidecar_paths(&uploaded_dir, &mut paths);
    paths.sort_by(|left, right| {
        transcript_path_sort_key(left).cmp(&transcript_path_sort_key(right))
    });

    paths
        .iter()
        .filter_map(|path| recent_transcript_from_sidecar(path))
        .collect()
}

/// Non-recursive: collect non-error `.txt` sidecars directly inside `dir`.
fn collect_day_sidecar_paths(dir: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_type().map(|kind| kind.is_file()).unwrap_or(false)
            && is_transcript_sidecar(&path)
        {
            paths.push(path);
        }
    }
}

pub fn recent_transcripts_from_save_root(root: &Path, limit: usize) -> Vec<RecentTranscript> {
    if limit == 0 {
        return Vec::new();
    }

    let mut days = collect_day_keys(root);
    // Newest day first.
    days.sort_by(|left, right| right.cmp(left));

    let _dataless_guard = DatalessMaterializationGuard::disabled();
    let mut transcripts = Vec::new();
    for day in days {
        if transcripts.len() >= limit {
            break;
        }
        // Merge both sources for this day before sorting, mirroring
        // `transcripts_for_day_from_save_root`, so the within-day order (and
        // any `limit` truncation) spans top-level and `uploaded` together.
        let mut paths = Vec::new();
        collect_day_sidecar_paths(&root.join(&day), &mut paths);
        collect_day_sidecar_paths(&root.join("uploaded").join(&day), &mut paths);
        // Newest within the day first.
        paths.sort_by(|left, right| {
            transcript_path_sort_key(right).cmp(&transcript_path_sort_key(left))
        });
        for path in paths {
            if transcripts.len() >= limit {
                break;
            }
            if let Some(transcript) = recent_transcript_from_sidecar(&path) {
                transcripts.push(transcript);
            }
        }
    }
    transcripts
}

/// Unique compact `YYYYMMDD` day codes that have a folder under `root`
/// (top-level) or `root/uploaded`. Order is unspecified (caller sorts).
fn collect_day_keys(root: &Path) -> Vec<String> {
    let mut days = BTreeSet::new();
    push_day_keys(root, &mut days);
    push_day_keys(&root.join("uploaded"), &mut days);
    days.into_iter().collect()
}

/// Inserts each `YYYYMMDD`-named subdirectory of `parent` into `days`.
fn push_day_keys(parent: &Path, days: &mut BTreeSet<String>) {
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_str().unwrap_or_default();
        if name.len() == 8 && name.bytes().all(|byte| byte.is_ascii_digit()) {
            days.insert(name.to_string());
        }
    }
}

#[cfg(target_os = "macos")]
struct DatalessMaterializationGuard {
    previous_policy: Option<std::ffi::c_int>,
}

#[cfg(target_os = "macos")]
impl DatalessMaterializationGuard {
    fn disabled() -> Self {
        const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: std::ffi::c_int = 3;
        const IOPOL_SCOPE_THREAD: std::ffi::c_int = 1;
        const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: std::ffi::c_int = 1;

        let previous_policy = unsafe {
            getiopolicy_np(
                IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
                IOPOL_SCOPE_THREAD,
            )
        };
        if previous_policy < 0 {
            return Self {
                previous_policy: None,
            };
        }

        let changed = unsafe {
            setiopolicy_np(
                IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
                IOPOL_SCOPE_THREAD,
                IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
            )
        } == 0;

        Self {
            previous_policy: changed.then_some(previous_policy),
        }
    }
}

#[cfg(target_os = "macos")]
impl Drop for DatalessMaterializationGuard {
    fn drop(&mut self) {
        const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: std::ffi::c_int = 3;
        const IOPOL_SCOPE_THREAD: std::ffi::c_int = 1;

        if let Some(previous_policy) = self.previous_policy {
            let _ = unsafe {
                setiopolicy_np(
                    IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
                    IOPOL_SCOPE_THREAD,
                    previous_policy,
                )
            };
        }
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn getiopolicy_np(iotype: std::ffi::c_int, scope: std::ffi::c_int) -> std::ffi::c_int;
    fn setiopolicy_np(
        iotype: std::ffi::c_int,
        scope: std::ffi::c_int,
        policy: std::ffi::c_int,
    ) -> std::ffi::c_int;
}

#[cfg(not(target_os = "macos"))]
struct DatalessMaterializationGuard;

#[cfg(not(target_os = "macos"))]
impl DatalessMaterializationGuard {
    fn disabled() -> Self {
        Self
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

    #[test]
    fn pinned_device_mismatch_returns_false_when_label_matches_after_id_drift() {
        // Stable ids embed the cpal enumeration index. After USB re-enumeration
        // the same physical mic has a new stable id but the label is unchanged.
        // Label match alone is enough to say we're on the pinned device.
        assert!(!pinned_device_mismatch(
            "input-3-by-v",
            "BY-V",
            "input-5-by-v",
            "BY-V",
            false,
        ));
    }

    #[test]
    fn pinned_device_mismatch_returns_true_when_label_differs() {
        assert!(pinned_device_mismatch(
            "input-3-by-v",
            "BY-V",
            "default",
            "System Default",
            true,
        ));
    }

    #[test]
    fn pinned_device_mismatch_returns_true_when_resolver_explicitly_fell_back() {
        // Even with matching labels, trust the resolver's explicit fallback signal.
        assert!(pinned_device_mismatch(
            "input-3-by-v",
            "BY-V",
            "input-3-by-v",
            "BY-V",
            true,
        ));
    }

    #[test]
    fn pinned_device_mismatch_returns_false_when_ids_match() {
        assert!(!pinned_device_mismatch(
            "input-3-by-v",
            "BY-V",
            "input-3-by-v",
            "BY-V",
            false,
        ));
    }

    #[test]
    fn pinned_device_mismatch_returns_true_when_pinned_label_empty_and_ids_differ() {
        // Legacy settings might not have a label. Without a label, fall back
        // to id equality only.
        assert!(pinned_device_mismatch(
            "input-3",
            "",
            "input-5",
            "Some Mic",
            false,
        ));
    }

    fn settings_with_priority(
        selected_id: &str,
        selected_label: &str,
        priority: Vec<(&str, &str)>,
    ) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.selected_microphone = selected_id.to_string();
        settings.selected_microphone_label = selected_label.to_string();
        settings.microphone_priority = priority
            .into_iter()
            .map(|(id, label)| crate::settings::MicrophonePriorityEntry {
                id: id.to_string(),
                label: label.to_string(),
            })
            .collect();
        settings
    }

    #[test]
    fn reconcile_device_id_in_settings_rewrites_selected_and_priority_entries() {
        let mut settings = settings_with_priority(
            "input-3-by-v",
            "BY-V",
            vec![("input-3-by-v", "BY-V"), ("input-7-airpods", "AirPods")],
        );

        let changed = reconcile_device_id_in_settings(
            &mut settings,
            "input-3-by-v",
            "input-5-by-v",
            "BY-V",
        );

        assert!(changed);
        assert_eq!(settings.selected_microphone, "input-5-by-v");
        assert_eq!(settings.selected_microphone_label, "BY-V");
        assert_eq!(settings.microphone_priority[0].id, "input-5-by-v");
        assert_eq!(settings.microphone_priority[1].id, "input-7-airpods");
    }

    #[test]
    fn reconcile_device_id_in_settings_skips_when_label_does_not_match() {
        // If the resolver returned a different label for the same id, we
        // can't be sure it's the same physical device — leave settings alone.
        let mut settings = settings_with_priority(
            "input-3-by-v",
            "BY-V",
            vec![("input-3-by-v", "BY-V")],
        );

        let changed = reconcile_device_id_in_settings(
            &mut settings,
            "input-3-by-v",
            "input-5-other",
            "Some Other Mic",
        );

        assert!(!changed);
        assert_eq!(settings.selected_microphone, "input-3-by-v");
        assert_eq!(settings.microphone_priority[0].id, "input-3-by-v");
    }

    #[test]
    fn reconcile_device_id_in_settings_noop_when_ids_equal() {
        let mut settings = settings_with_priority(
            "input-3-by-v",
            "BY-V",
            vec![("input-3-by-v", "BY-V")],
        );

        let changed = reconcile_device_id_in_settings(
            &mut settings,
            "input-3-by-v",
            "input-3-by-v",
            "BY-V",
        );

        assert!(!changed);
    }

    #[test]
    fn reconcile_device_id_in_settings_noop_when_resolved_label_empty() {
        // No label means we can't safely identify the device; skip.
        let mut settings = settings_with_priority(
            "input-3-by-v",
            "BY-V",
            vec![("input-3-by-v", "BY-V")],
        );

        let changed = reconcile_device_id_in_settings(
            &mut settings,
            "input-3-by-v",
            "input-5-by-v",
            "",
        );

        assert!(!changed);
        assert_eq!(settings.selected_microphone, "input-3-by-v");
    }

    #[test]
    fn transcript_days_counts_sidecars_per_day_including_uploaded() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let day1 = root.join("20260510");
        std::fs::create_dir_all(&day1).expect("day1");
        std::fs::write(day1.join("010203.txt"), "morning").expect("t1");
        std::fs::write(day1.join("110203.txt"), "noon").expect("t2");
        // error / empty-by-name sidecars must not be counted as transcripts.
        std::fs::write(day1.join("120000.error.txt"), "boom").expect("err");
        // audio without a transcript must not be counted.
        std::fs::write(day1.join("130000.m4a"), b"audio").expect("audio");

        let day2 = root.join("20260511");
        std::fs::create_dir_all(&day2).expect("day2");
        std::fs::write(day2.join("090000.txt"), "second day").expect("t3");

        // Uploaded files live under uploaded/YYYYMMDD and merge into the same day bucket.
        let uploaded = root.join("uploaded").join("20260511");
        std::fs::create_dir_all(&uploaded).expect("uploaded");
        std::fs::write(uploaded.join("memo.txt"), "imported").expect("t4");

        // A non-date directory must be ignored.
        std::fs::create_dir_all(root.join("notes")).expect("notes");

        let days = transcript_days_from_save_root(root);

        assert_eq!(
            days,
            vec![
                TranscriptDay { day: "2026-05-10".to_string(), count: 2 },
                TranscriptDay { day: "2026-05-11".to_string(), count: 2 },
            ]
        );
    }

    #[test]
    fn transcript_days_excludes_empty_sidecars_to_match_day_view() {
        // Suppressed (low-confidence) transcripts are written as a lone "\n"
        // (one byte) by `write_text_sidecar`. The size-based count must exclude
        // them, otherwise a day would look selectable but open to "No
        // transcripts for this day".
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        // Day with ONLY a one-byte suppressed stub -> excluded entirely (count 0).
        let suppressed_only = root.join("20260512");
        std::fs::create_dir_all(&suppressed_only).expect("suppressed_only");
        std::fs::write(suppressed_only.join("100000.txt"), "\n").expect("suppressed");

        // Day with one suppressed stub + one real sidecar -> count == 1.
        let mixed = root.join("20260513");
        std::fs::create_dir_all(&mixed).expect("mixed");
        std::fs::write(mixed.join("090000.txt"), "\n").expect("suppressed");
        std::fs::write(mixed.join("100000.txt"), "real transcript\n").expect("real");

        let days = transcript_days_from_save_root(root);

        assert_eq!(
            days,
            vec![TranscriptDay { day: "2026-05-13".to_string(), count: 1 }]
        );
    }

    #[test]
    fn transcripts_for_day_reads_only_that_day_including_uploaded() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let target = root.join("20260510");
        std::fs::create_dir_all(&target).expect("target");
        std::fs::write(target.join("090000.txt"), "morning").expect("t1");
        std::fs::write(target.join("180000.txt"), "evening").expect("t2");
        std::fs::write(target.join("190000.txt"), "  \n").expect("empty");

        let uploaded = root.join("uploaded").join("20260510");
        std::fs::create_dir_all(&uploaded).expect("uploaded");
        std::fs::write(uploaded.join("memo.txt"), "imported").expect("t3");

        let other = root.join("20260511");
        std::fs::create_dir_all(&other).expect("other");
        std::fs::write(other.join("090000.txt"), "other day").expect("t4");

        let result = transcripts_for_day_from_save_root(root, "2026-05-10", false);
        let texts: Vec<&str> = result.iter().map(|item| item.text.as_str()).collect();

        // Empty sidecar skipped; other day excluded; uploaded merged in.
        assert_eq!(result.len(), 3);
        assert!(texts.contains(&"morning"));
        assert!(texts.contains(&"evening"));
        assert!(texts.contains(&"imported"));
        assert!(!texts.contains(&"other day"));

        // Ascending (oldest-first): morning (09:00) precedes evening (18:00).
        // The uploaded "imported" entry sorts by full path, so only assert the
        // two same-folder entries' relative order.
        assert!(
            texts.iter().position(|text| *text == "morning")
                < texts.iter().position(|text| *text == "evening")
        );
    }

    #[test]
    fn transcripts_for_day_rejects_malformed_day() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(transcripts_for_day_from_save_root(tmp.path(), "20260510", false).is_empty());
        assert!(transcripts_for_day_from_save_root(tmp.path(), "not-a-day", false).is_empty());
    }

    #[test]
    fn transcripts_for_day_download_flag_is_equivalent_for_local_files() {
        // For materialized (non-evicted) files the dataless guard has no effect,
        // so the on-navigation (download=false) and reload (download=true) reads
        // return the same entries. The flag only matters for iCloud-evicted files.
        let tmp = tempfile::tempdir().expect("tempdir");
        let day = tmp.path().join("20260510");
        std::fs::create_dir_all(&day).expect("day");
        std::fs::write(day.join("090000.txt"), "morning\n").expect("t1");
        std::fs::write(day.join("180000.txt"), "evening\n").expect("t2");

        let local = transcripts_for_day_from_save_root(tmp.path(), "2026-05-10", false);
        let downloaded = transcripts_for_day_from_save_root(tmp.path(), "2026-05-10", true);
        assert_eq!(local, downloaded);
        assert_eq!(local.len(), 2);
    }

    #[test]
    fn recent_transcripts_orders_newest_day_first_across_buckets() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let older = root.join("20260510");
        std::fs::create_dir_all(&older).expect("older");
        std::fs::write(older.join("090000.txt"), "older day").expect("t1");

        let newer = root.join("20260512");
        std::fs::create_dir_all(&newer).expect("newer");
        std::fs::write(newer.join("080000.txt"), "newer morning").expect("t2");
        std::fs::write(newer.join("200000.txt"), "newer evening").expect("t3");

        let all = recent_transcripts_from_save_root(root, 10);
        let texts: Vec<&str> = all.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(texts, vec!["newer evening", "newer morning", "older day"]);

        // Limit stops once filled, newest first.
        let limited = recent_transcripts_from_save_root(root, 1);
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].text, "newer evening");
    }

    #[test]
    fn recent_transcripts_merges_uploaded_into_same_day_before_sorting() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let root = tmp.path();

        let top = root.join("20260512");
        std::fs::create_dir_all(&top).expect("top day");
        std::fs::write(top.join("090000.txt"), "top morning").expect("t1");

        let uploaded = root.join("uploaded").join("20260512");
        std::fs::create_dir_all(&uploaded).expect("uploaded day");
        std::fs::write(uploaded.join("200000.txt"), "uploaded evening").expect("t2");

        let all = recent_transcripts_from_save_root(root, 10);
        let texts: Vec<&str> = all.iter().map(|item| item.text.as_str()).collect();
        assert_eq!(texts.len(), 2);
        assert!(texts.contains(&"top morning"));
        assert!(texts.contains(&"uploaded evening"));
        // Within the day, both sources are merged before sorting, so the
        // 20:00 uploaded entry precedes the 09:00 top-level one.
        assert!(
            texts.iter().position(|t| *t == "uploaded evening")
                < texts.iter().position(|t| *t == "top morning")
        );

        // The merge happens before truncation: limit 1 keeps the newest of
        // the merged day, which lives in `uploaded/`.
        let limited = recent_transcripts_from_save_root(root, 1);
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].text, "uploaded evening");
    }

    #[test]
    fn recent_transcripts_fills_limit_after_skipping_empty_sidecars() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let day = tmp.path().join("20260528");
        std::fs::create_dir_all(&day).expect("day dir");
        std::fs::write(day.join("000003.txt"), "   \n").expect("empty transcript");
        std::fs::write(day.join("000002.txt"), "second newest").expect("second transcript");
        std::fs::write(day.join("000001.txt"), "oldest").expect("oldest transcript");

        let transcripts = recent_transcripts_from_save_root(tmp.path(), 2);

        assert_eq!(transcripts.len(), 2);
        assert_eq!(transcripts[0].text, "second newest");
        assert_eq!(transcripts[1].text, "oldest");
    }
}
