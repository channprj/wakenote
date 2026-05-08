use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::audio::{LevelMonitor, LevelSnapshot, list_input_devices};
use crate::capture::{CaptureController, CaptureControllerConfig, CaptureControllerEvent};
use crate::live_capture::AudioFrame;
use crate::models::{ModelDescriptor, ModelStore, default_model_registry};
use crate::persistence::{AppPersistence, PersistenceError};
use crate::queue::{BacklogScan, QueueSnapshot, TranscriptionQueue};
use crate::settings::{AppSettings, SettingsPatch, expand_user_path};
use crate::transcription::{
    Transcriber, TranscriptionJobOutcome, TranscriptionWorker, WhisperTranscriber, apply_outcome,
};

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
            tooltip: "Sagwan: Idle",
        },
        TrayState::Listening => TrayPresentation {
            rgba: [0, 71, 171, 255],
            tooltip: "Sagwan: Listening",
        },
        TrayState::Recording => TrayPresentation {
            rgba: [22, 163, 74, 255],
            tooltip: "Sagwan: Recording",
        },
        TrayState::Transcribing => TrayPresentation {
            rgba: [217, 119, 6, 255],
            tooltip: "Sagwan: Transcribing",
        },
        TrayState::Paused => TrayPresentation {
            rgba: [71, 85, 105, 255],
            tooltip: "Sagwan: Paused",
        },
        TrayState::Error => TrayPresentation {
            rgba: [220, 38, 38, 255],
            tooltip: "Sagwan: Error",
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

#[derive(Debug, Clone)]
pub struct StartedTranscriptionJob {
    pub job: crate::queue::QueueJob,
    pub model_directory: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedModelDownload {
    pub model_directory: std::path::PathBuf,
    pub model: ModelDescriptor,
    pub registry: Vec<ModelDescriptor>,
}

#[derive(Debug)]
pub struct AppBackend {
    settings: AppSettings,
    queue: TranscriptionQueue,
    capture: Option<CaptureController>,
    level_monitor: LevelMonitor,
    active_microphone_label: Option<String>,
    microphone_warning: Option<String>,
    persistence: Option<AppPersistence>,
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
        })
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.clone()
    }

    pub fn update_settings(&mut self, patch: SettingsPatch) -> AppSettings {
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
        let store = self.model_store();
        let mut models: Vec<ModelDescriptor> = store
            .load_model_registry()
            .unwrap_or_else(|_| default_model_registry())
            .into_values()
            .collect();
        let _ = store.refresh_statuses(&mut models);
        models
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
        let store = self.model_store();
        let registry = store
            .load_model_registry()
            .map_err(|error| error.to_string())?;
        let model = registry
            .get(model_id)
            .ok_or_else(|| format!("unknown model {model_id}"))?;
        store
            .download_model(model)
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

        store
            .cancel_download(model_id)
            .map_err(|error| error.to_string())?;
        Ok(self.model_registry())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<Vec<ModelDescriptor>, String> {
        let store = self.model_store();
        store
            .delete_model(model_id)
            .map_err(|error| error.to_string())?;
        Ok(self.model_registry())
    }

    pub fn queue_snapshot(&self) -> QueueSnapshot {
        self.queue.snapshot()
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
        self.capture = None;
        self.active_microphone_label = None;
        Ok(self.app_status())
    }

    pub fn capture_start_failed(&mut self, warning: impl Into<String>) -> AppStatus {
        self.capture = None;
        self.active_microphone_label = None;
        self.microphone_warning = Some(warning.into());
        self.app_status()
    }

    pub fn process_audio_frame(&mut self, frame: AudioFrame) -> Result<AppStatus, String> {
        self.level_monitor.observe_samples(&frame.samples);
        let events = self
            .capture
            .as_mut()
            .ok_or_else(|| "capture session is not running".to_string())?
            .process_samples(&frame.samples, frame.duration_ms)
            .map_err(|error| error.to_string())?;
        self.handle_capture_events(events);
        Ok(self.app_status())
    }

    pub fn process_audio_samples_for_test(
        &mut self,
        samples: &[f32],
        duration_ms: u64,
    ) -> Result<AppStatus, String> {
        self.process_audio_frame(AudioFrame {
            samples: samples.to_vec(),
            duration_ms,
        })
    }

    pub fn enqueue_audio_file(
        &mut self,
        audio_path: impl Into<std::path::PathBuf>,
        model_id: Option<String>,
    ) -> QueueSnapshot {
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
        self.queue.cancel_current("cancelled by user")?;
        self.persist_queue();
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

        let worker = TranscriptionWorker::new(transcriber);
        worker
            .process_next(&mut self.queue)
            .map_err(|error| error.to_string())?;
        self.persist_queue();
        Ok(self.queue.snapshot())
    }

    pub fn process_pending_transcriptions_with<T: Clone + Transcriber>(
        &mut self,
        transcriber: T,
    ) -> Result<QueueSnapshot, String> {
        while let Some(started) = self.start_next_transcription_job() {
            let worker = TranscriptionWorker::new(transcriber.clone());
            let outcome = worker
                .process_started_job(&started.job)
                .map_err(|error| error.to_string())?;
            self.finish_transcription_job(outcome)?;
        }

        Ok(self.queue.snapshot())
    }

    pub fn should_process_transcriptions(&self) -> bool {
        !self.settings.pause_all
            && self.settings.transcription_enabled
            && self.queue.snapshot().pending_count > 0
    }

    pub fn start_next_transcription_job(&mut self) -> Option<StartedTranscriptionJob> {
        if self.settings.pause_all || !self.settings.transcription_enabled {
            return None;
        }

        let job = self.queue.start_next()?;
        self.persist_queue();
        Some(StartedTranscriptionJob {
            job,
            model_directory: self.model_directory_path(),
        })
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
        list_input_devices()
            .into_iter()
            .map(|device| MicrophoneDevice {
                fallback: device.id == "default" && self.settings.selected_microphone != "default",
                id: device.id,
                label: device.label,
                available: device.available,
            })
            .collect()
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
            runtime_warning: None,
            threshold_dbfs: self.settings.threshold_dbfs,
            level: self.level_monitor.snapshot(),
            queue,
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
    }

    fn handle_capture_events(&mut self, events: Vec<CaptureControllerEvent>) {
        for event in events {
            match event {
                CaptureControllerEvent::ChunkCompleted { chunk } => {
                    if self.settings.transcription_enabled {
                        self.queue
                            .enqueue_file(chunk.audio_path, self.settings.selected_model.clone());
                    }
                }
            }
        }

        self.persist_queue();
    }
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
    if transcribing {
        return TrayState::Transcribing;
    }

    if recording {
        return TrayState::Recording;
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
