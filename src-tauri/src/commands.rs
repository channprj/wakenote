use serde::{Deserialize, Serialize};

use crate::audio::list_input_devices;
use crate::models::{ModelDescriptor, ModelStore, default_model_registry};
use crate::persistence::{AppPersistence, PersistenceError};
use crate::queue::{BacklogScan, QueueSnapshot, TranscriptionQueue};
use crate::settings::{AppSettings, SettingsPatch};

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
    pub active_model: String,
    pub active_microphone: String,
    pub threshold_dbfs: f32,
    pub queue: QueueSnapshot,
}

#[derive(Debug)]
pub struct AppBackend {
    settings: AppSettings,
    queue: TranscriptionQueue,
    persistence: Option<AppPersistence>,
}

impl Default for AppBackend {
    fn default() -> Self {
        Self {
            settings: AppSettings::default(),
            queue: TranscriptionQueue::new(),
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
            persistence: Some(persistence),
        })
    }

    pub fn settings(&self) -> AppSettings {
        self.settings.clone()
    }

    pub fn update_settings(&mut self, patch: SettingsPatch) -> AppSettings {
        self.settings.apply_patch(patch);
        self.persist_settings();
        self.settings.clone()
    }

    pub fn model_registry(&self) -> Vec<ModelDescriptor> {
        let mut models: Vec<ModelDescriptor> = default_model_registry().into_values().collect();
        let store = ModelStore::new(&self.settings.model_directory);
        let _ = store.refresh_statuses(&mut models);
        models
    }

    pub fn verify_model(&self, model_id: &str) -> Result<Vec<ModelDescriptor>, String> {
        if !default_model_registry().contains_key(model_id) {
            return Err(format!("unknown model {model_id}"));
        }

        Ok(self.model_registry())
    }

    pub fn delete_model(&self, model_id: &str) -> Result<Vec<ModelDescriptor>, String> {
        let store = ModelStore::new(&self.settings.model_directory);
        store
            .delete_model(model_id)
            .map_err(|error| error.to_string())?;
        Ok(self.model_registry())
    }

    pub fn queue_snapshot(&self) -> QueueSnapshot {
        self.queue.snapshot()
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
        let scan = BacklogScan::scan(save_root.as_ref())?;
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
        let tray_state = derive_tray_state(mode, queue.running_count > 0);

        AppStatus {
            mode,
            tray_state,
            active_model: self.settings.selected_model.clone(),
            active_microphone: self.settings.selected_microphone_label.clone(),
            threshold_dbfs: self.settings.threshold_dbfs,
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

fn derive_tray_state(mode: AppMode, transcribing: bool) -> TrayState {
    if transcribing {
        return TrayState::Transcribing;
    }

    match mode {
        AppMode::RecordingAndTranscription | AppMode::RecordingOnly => TrayState::Listening,
        AppMode::TranscriptionOnly => TrayState::Idle,
        AppMode::Paused => TrayState::Paused,
    }
}
