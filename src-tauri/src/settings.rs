use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    M4a,
    Wav,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FloatingOverlayPosition {
    Off,
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionLanguage {
    Auto,
    Ko,
    En,
    Ja,
    Zh,
    Es,
    Fr,
    De,
}

impl TranscriptionLanguage {
    pub fn whisper_code(self) -> Option<&'static str> {
        match self {
            Self::Auto => None,
            Self::Ko => Some("ko"),
            Self::En => Some("en"),
            Self::Ja => Some("ja"),
            Self::Zh => Some("zh"),
            Self::Es => Some("es"),
            Self::Fr => Some("fr"),
            Self::De => Some("de"),
        }
    }
}

impl Default for TranscriptionLanguage {
    fn default() -> Self {
        Self::Ko
    }
}

/// One entry in the microphone priority list. Persisted alongside the legacy
/// `selected_microphone[_label]` fields so the UI can render an unplugged
/// device's name even when it isn't currently enumerable from cpal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrophonePriorityEntry {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub recording_enabled: bool,
    pub transcription_enabled: bool,
    pub transcription_language: TranscriptionLanguage,
    pub suppress_low_confidence_transcripts: bool,
    pub pause_all: bool,
    pub selected_microphone: String,
    pub selected_microphone_label: String,
    /// Ordered list of preferred microphones. Position 0 is the top-priority
    /// device that capture tries first and that the watchdog re-attempts
    /// every [`TOP_PRIORITY_RECHECK`](crate::audio::TOP_PRIORITY_RECHECK)
    /// when the active device is somewhere further down the list.
    #[serde(default)]
    pub microphone_priority: Vec<MicrophonePriorityEntry>,
    pub save_root: String,
    pub save_root_confirmed: bool,
    pub audio_format: AudioFormat,
    pub threshold_dbfs: f32,
    pub calibration_completed: bool,
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
    pub start_live_input_on_launch: bool,
    pub show_dock_icon: bool,
    pub show_tray_icon: bool,
    pub show_floating_overlay: bool,
    pub floating_overlay_position: FloatingOverlayPosition,
    pub theme_primary_color: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub recording_enabled: Option<bool>,
    pub transcription_enabled: Option<bool>,
    pub transcription_language: Option<TranscriptionLanguage>,
    pub suppress_low_confidence_transcripts: Option<bool>,
    pub pause_all: Option<bool>,
    pub selected_microphone: Option<String>,
    pub selected_microphone_label: Option<String>,
    pub microphone_priority: Option<Vec<MicrophonePriorityEntry>>,
    pub save_root: Option<String>,
    pub audio_format: Option<AudioFormat>,
    pub threshold_dbfs: Option<f32>,
    pub calibration_completed: Option<bool>,
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
    pub start_live_input_on_launch: Option<bool>,
    pub show_dock_icon: Option<bool>,
    pub show_tray_icon: Option<bool>,
    pub show_floating_overlay: Option<bool>,
    pub floating_overlay_position: Option<FloatingOverlayPosition>,
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

pub fn default_microphone_priority() -> Vec<MicrophonePriorityEntry> {
    vec![MicrophonePriorityEntry {
        id: "default".to_string(),
        label: "System Default".to_string(),
    }]
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
        .is_some_and(|value| value != &settings.selected_microphone)
        || patch
            .microphone_priority
            .as_ref()
            .is_some_and(|list| top_priority_id(list) != settings.selected_microphone);

    match (currently_running, should_run, microphone_changed) {
        (false, true, _) => LiveCaptureRuntimeAction::Start,
        (true, false, _) => LiveCaptureRuntimeAction::Stop,
        (true, true, true) => LiveCaptureRuntimeAction::Restart,
        _ => LiveCaptureRuntimeAction::Unchanged,
    }
}

/// Top-of-list device id for a priority list, or "default" when the list is
/// empty (which only happens transiently during user edits / migration).
pub fn top_priority_id(list: &[MicrophonePriorityEntry]) -> String {
    list.first()
        .map(|entry| entry.id.clone())
        .unwrap_or_else(|| "default".to_string())
}

pub fn top_priority_label(list: &[MicrophonePriorityEntry]) -> String {
    list.first()
        .map(|entry| entry.label.clone())
        .unwrap_or_else(|| "System Default".to_string())
}

pub fn live_capture_should_run(settings: &AppSettings) -> bool {
    settings.recording_enabled && !settings.pause_all
}

pub fn live_capture_should_start_on_launch(settings: &AppSettings) -> bool {
    settings.start_live_input_on_launch && live_capture_should_run(settings)
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

fn clamp_threshold_dbfs(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-90.0, -10.0)
    } else {
        AppSettings::default().threshold_dbfs
    }
}

fn clamp_ms(value: u64, min: u64, max: u64) -> u64 {
    value.clamp(min, max)
}

impl AppSettings {
    /// Reconcile [`Self::microphone_priority`] with the legacy
    /// `selected_microphone[_label]` fields. The priority list is the source of
    /// truth: after this call, `priority[0]` and the legacy single-mic fields
    /// always match. Older persisted settings without a priority list (or with
    /// a list that no longer matches the legacy fields, e.g. after a migration
    /// from a release that only wrote the single field) get repaired here.
    pub fn normalize_microphone_priority(&mut self) {
        // Drop any empty / duplicate entries from the priority list first.
        let mut seen = std::collections::HashSet::new();
        self.microphone_priority
            .retain(|entry| !entry.id.is_empty() && seen.insert(entry.id.clone()));

        if self.microphone_priority.is_empty() {
            // Legacy / fresh install: build a single-entry list from the
            // selected-microphone fields.
            let id = if self.selected_microphone.is_empty() {
                "default".to_string()
            } else {
                self.selected_microphone.clone()
            };
            let label = if self.selected_microphone_label.is_empty() {
                if id == "default" {
                    "System Default".to_string()
                } else {
                    id.clone()
                }
            } else {
                self.selected_microphone_label.clone()
            };
            self.microphone_priority
                .push(MicrophonePriorityEntry { id, label });
        }

        let top = self
            .microphone_priority
            .first()
            .cloned()
            .expect("priority list is non-empty after repair");
        self.selected_microphone = top.id;
        self.selected_microphone_label = top.label;
    }

    pub fn apply_patch(&mut self, patch: SettingsPatch) {
        if let Some(value) = patch.recording_enabled {
            self.recording_enabled = value;
        }
        if let Some(value) = patch.transcription_enabled {
            self.transcription_enabled = value;
        }
        if let Some(value) = patch.transcription_language {
            self.transcription_language = value;
        }
        if let Some(value) = patch.suppress_low_confidence_transcripts {
            self.suppress_low_confidence_transcripts = value;
        }
        if let Some(value) = patch.pause_all {
            self.pause_all = value;
        }
        // Legacy single-mic fields. If the patch carries only these (no
        // explicit microphone_priority), splice the new value into position 0
        // of the priority list so the priority stays in sync.
        let legacy_mic_changed =
            patch.selected_microphone.is_some() || patch.selected_microphone_label.is_some();
        if let Some(value) = patch.selected_microphone {
            self.selected_microphone = value;
        }
        if let Some(value) = patch.selected_microphone_label {
            self.selected_microphone_label = value;
        }
        if legacy_mic_changed && patch.microphone_priority.is_none() {
            // Move/insert the legacy selection to the top of the priority list.
            let new_top = MicrophonePriorityEntry {
                id: self.selected_microphone.clone(),
                label: self.selected_microphone_label.clone(),
            };
            self.microphone_priority
                .retain(|entry| entry.id != new_top.id);
            self.microphone_priority.insert(0, new_top);
        }
        if let Some(list) = patch.microphone_priority {
            self.microphone_priority = list;
        }
        self.normalize_microphone_priority();
        if let Some(value) = patch.save_root {
            self.save_root_confirmed = !value.trim().is_empty();
            self.save_root = value;
        }
        if let Some(value) = patch.audio_format {
            self.audio_format = value;
        }
        if let Some(value) = patch.threshold_dbfs {
            self.threshold_dbfs = clamp_threshold_dbfs(value);
        }
        if let Some(value) = patch.calibration_completed {
            self.calibration_completed = value;
        }
        if let Some(value) = patch.attack_ms {
            self.attack_ms = clamp_ms(value, 50, 2_000);
        }
        if let Some(value) = patch.release_ms {
            self.release_ms = clamp_ms(value, 250, 5_000);
        }
        if let Some(value) = patch.pre_roll_ms {
            self.pre_roll_ms = clamp_ms(value, 0, 1_500);
        }
        if let Some(value) = patch.post_roll_ms {
            self.post_roll_ms = clamp_ms(value, 0, 2_000);
        }
        if let Some(value) = patch.min_chunk_ms {
            self.min_chunk_ms = clamp_ms(value, 100, 5_000);
        }
        if let Some(value) = patch.max_chunk_ms {
            self.max_chunk_ms = clamp_ms(value, 10_000, 900_000);
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
        if let Some(value) = patch.start_live_input_on_launch {
            self.start_live_input_on_launch = value;
        }
        if let Some(value) = patch.show_dock_icon {
            self.show_dock_icon = value;
        }
        if let Some(value) = patch.show_tray_icon {
            self.show_tray_icon = value;
        }
        if let Some(value) = patch.show_floating_overlay {
            self.show_floating_overlay = value;
        }
        if let Some(value) = patch.floating_overlay_position {
            self.floating_overlay_position = value;
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
            transcription_language: TranscriptionLanguage::Ko,
            suppress_low_confidence_transcripts: true,
            pause_all: false,
            selected_microphone: "default".to_string(),
            selected_microphone_label: "System Default".to_string(),
            microphone_priority: default_microphone_priority(),
            save_root: "~/Documents/WakeNote".to_string(),
            save_root_confirmed: false,
            audio_format: AudioFormat::M4a,
            threshold_dbfs: -60.0,
            calibration_completed: false,
            attack_ms: 200,
            release_ms: 1_500,
            pre_roll_ms: 400,
            post_roll_ms: 400,
            min_chunk_ms: 500,
            max_chunk_ms: 120_000,
            selected_model: "whisper-medium".to_string(),
            model_directory: "~/Library/Application Support/WakeNote/models".to_string(),
            vad_enabled: false,
            launch_at_login: false,
            start_live_input_on_launch: true,
            show_dock_icon: true,
            show_tray_icon: true,
            show_floating_overlay: true,
            floating_overlay_position: FloatingOverlayPosition::Top,
            theme_primary_color: "#000".to_string(),
        }
    }
}
