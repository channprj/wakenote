use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    M4a,
    Wav,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub recording_enabled: bool,
    pub transcription_enabled: bool,
    pub pause_all: bool,
    pub selected_microphone: String,
    pub selected_microphone_label: String,
    pub save_root: String,
    pub audio_format: AudioFormat,
    pub threshold_dbfs: f32,
    pub attack_ms: u64,
    pub release_ms: u64,
    pub pre_roll_ms: u64,
    pub post_roll_ms: u64,
    pub min_chunk_ms: u64,
    pub max_chunk_ms: u64,
    pub selected_model: String,
    pub model_directory: String,
    pub vad_enabled: bool,
    pub launch_at_login: bool,
    pub show_tray_icon: bool,
    pub show_floating_overlay: bool,
    pub theme_primary_color: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub recording_enabled: Option<bool>,
    pub transcription_enabled: Option<bool>,
    pub pause_all: Option<bool>,
    pub selected_microphone: Option<String>,
    pub selected_microphone_label: Option<String>,
    pub save_root: Option<String>,
    pub audio_format: Option<AudioFormat>,
    pub threshold_dbfs: Option<f32>,
    pub attack_ms: Option<u64>,
    pub release_ms: Option<u64>,
    pub pre_roll_ms: Option<u64>,
    pub post_roll_ms: Option<u64>,
    pub min_chunk_ms: Option<u64>,
    pub max_chunk_ms: Option<u64>,
    pub selected_model: Option<String>,
    pub model_directory: Option<String>,
    pub vad_enabled: Option<bool>,
    pub launch_at_login: Option<bool>,
    pub show_tray_icon: Option<bool>,
    pub show_floating_overlay: Option<bool>,
    pub theme_primary_color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchAtLoginAction {
    Enable,
    Disable,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveCaptureRuntimeAction {
    Start,
    Stop,
    Restart,
    Unchanged,
}

pub fn launch_at_login_action_for_patch(
    settings: &AppSettings,
    patch: &SettingsPatch,
) -> LaunchAtLoginAction {
    match patch.launch_at_login {
        Some(true) if !settings.launch_at_login => LaunchAtLoginAction::Enable,
        Some(false) if settings.launch_at_login => LaunchAtLoginAction::Disable,
        _ => LaunchAtLoginAction::Unchanged,
    }
}

pub fn live_capture_runtime_action_for_patch(
    settings: &AppSettings,
    patch: &SettingsPatch,
) -> LiveCaptureRuntimeAction {
    let currently_running = live_capture_should_run(settings);
    let next_recording_enabled = patch
        .recording_enabled
        .unwrap_or(settings.recording_enabled);
    let next_pause_all = patch.pause_all.unwrap_or(settings.pause_all);
    let should_run = next_recording_enabled && !next_pause_all;
    let microphone_changed = patch
        .selected_microphone
        .as_ref()
        .is_some_and(|value| value != &settings.selected_microphone);

    match (currently_running, should_run, microphone_changed) {
        (false, true, _) => LiveCaptureRuntimeAction::Start,
        (true, false, _) => LiveCaptureRuntimeAction::Stop,
        (true, true, true) => LiveCaptureRuntimeAction::Restart,
        _ => LiveCaptureRuntimeAction::Unchanged,
    }
}

pub fn live_capture_should_run(settings: &AppSettings) -> bool {
    settings.recording_enabled && !settings.pause_all
}

pub fn expand_user_path(path: impl AsRef<str>) -> PathBuf {
    let path = path.as_ref();
    let Some(home) = std::env::var_os("HOME") else {
        return PathBuf::from(path);
    };

    if path == "~" {
        return PathBuf::from(home);
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return PathBuf::from(home).join(rest);
    }

    PathBuf::from(path)
}

impl AppSettings {
    pub fn apply_patch(&mut self, patch: SettingsPatch) {
        if let Some(value) = patch.recording_enabled {
            self.recording_enabled = value;
        }
        if let Some(value) = patch.transcription_enabled {
            self.transcription_enabled = value;
        }
        if let Some(value) = patch.pause_all {
            self.pause_all = value;
        }
        if let Some(value) = patch.selected_microphone {
            self.selected_microphone = value;
        }
        if let Some(value) = patch.selected_microphone_label {
            self.selected_microphone_label = value;
        }
        if let Some(value) = patch.save_root {
            self.save_root = value;
        }
        if let Some(value) = patch.audio_format {
            self.audio_format = value;
        }
        if let Some(value) = patch.threshold_dbfs {
            self.threshold_dbfs = value;
        }
        if let Some(value) = patch.attack_ms {
            self.attack_ms = value;
        }
        if let Some(value) = patch.release_ms {
            self.release_ms = value;
        }
        if let Some(value) = patch.pre_roll_ms {
            self.pre_roll_ms = value;
        }
        if let Some(value) = patch.post_roll_ms {
            self.post_roll_ms = value;
        }
        if let Some(value) = patch.min_chunk_ms {
            self.min_chunk_ms = value;
        }
        if let Some(value) = patch.max_chunk_ms {
            self.max_chunk_ms = value;
        }
        if let Some(value) = patch.selected_model {
            self.selected_model = value;
        }
        if let Some(value) = patch.model_directory {
            self.model_directory = value;
        }
        if let Some(value) = patch.vad_enabled {
            self.vad_enabled = value;
        }
        if let Some(value) = patch.launch_at_login {
            self.launch_at_login = value;
        }
        if let Some(value) = patch.show_tray_icon {
            self.show_tray_icon = value;
        }
        if let Some(value) = patch.show_floating_overlay {
            self.show_floating_overlay = value;
        }
        if let Some(value) = patch.theme_primary_color {
            self.theme_primary_color = value;
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            recording_enabled: true,
            transcription_enabled: true,
            pause_all: false,
            selected_microphone: "default".to_string(),
            selected_microphone_label: "System Default".to_string(),
            save_root: "~/Documents/Sagwan".to_string(),
            audio_format: AudioFormat::M4a,
            threshold_dbfs: -45.0,
            attack_ms: 300,
            release_ms: 1_500,
            pre_roll_ms: 300,
            post_roll_ms: 300,
            min_chunk_ms: 500,
            max_chunk_ms: 300_000,
            selected_model: "whisper-medium".to_string(),
            model_directory: "~/Library/Application Support/Sagwan/models".to_string(),
            vad_enabled: false,
            launch_at_login: false,
            show_tray_icon: true,
            show_floating_overlay: true,
            theme_primary_color: "#0047AB".to_string(),
        }
    }
}
