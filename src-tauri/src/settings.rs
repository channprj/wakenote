use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    M4a,
    Mp3,
    Wav,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FloatingOverlayPosition {
    Off,
    Top,
    Bottom,
}

pub const FLOATING_OVERLAY_FONT_SIZE_MIN_PX: u32 = 18;
pub const FLOATING_OVERLAY_FONT_SIZE_MAX_PX: u32 = 48;
pub const FLOATING_OVERLAY_BACKGROUND_OPACITY_MIN: u8 = 0;
pub const FLOATING_OVERLAY_BACKGROUND_OPACITY_MAX: u8 = 100;
pub const MIC_INPUT_VOLUME_MIN_PERCENT: u32 = 0;
pub const MIC_INPUT_VOLUME_MAX_PERCENT: u32 = 200;
pub const LLM_MAX_ITERATIONS_MIN: u8 = 1;
pub const LLM_MAX_ITERATIONS_MAX: u8 = 30;
pub const OPENROUTER_DEFAULT_MODEL_ID: &str = "z-ai/glm-5.2";

pub fn default_floating_overlay_font_size_px() -> u32 {
    24
}

pub fn default_floating_overlay_text_color() -> String {
    "#ffffff".to_string()
}

pub fn default_floating_overlay_background_color() -> String {
    "#050507".to_string()
}

pub fn default_floating_overlay_background_opacity() -> u8 {
    82
}

pub fn default_mic_input_volume_percent() -> u32 {
    100
}

pub fn default_openrouter_model() -> String {
    OPENROUTER_DEFAULT_MODEL_ID.to_string()
}

pub fn default_llm_max_iterations() -> u8 {
    3
}

pub fn default_llm_summary_prompt_template() -> String {
    [
        "You are WakeNote's transcript summary assistant.",
        "Do not invent facts that are not present in the transcript. Mark uncertainty clearly.",
        "Write in the transcript's dominant language; if Korean is present, write in Korean.",
        "",
        "Date range: {{date_range}}",
        "Selected transcripts: {{selected_count}}",
        "",
        "Return this structure:",
        "- One-line summary",
        "- Key points",
        "- Decisions",
        "- Action items",
        "- Open questions",
        "",
        "Transcript:",
        "{{transcripts}}",
    ]
    .join("\n")
}

pub fn default_llm_report_prompt_template() -> String {
    [
        "You are WakeNote's detailed transcript report writer.",
        "Use only the supplied transcript as evidence. Do not add unsupported assumptions.",
        "Preserve important time/source context where it helps the reader verify the report.",
        "",
        "Date range: {{date_range}}",
        "Selected transcripts: {{selected_count}}",
        "",
        "Write a Markdown report with these sections:",
        "# Summary",
        "# Context",
        "# Chronological Details",
        "# Main Discussion Points",
        "# Decisions",
        "# Action Items",
        "# Risks and Issues",
        "# Open Questions",
        "# Evidence Notes",
        "",
        "Transcript:",
        "{{transcripts}}",
    ]
    .join("\n")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FloatingOverlayCaptionStyle {
    pub font_size_px: u32,
    pub text_color: String,
    pub background_color: String,
    pub background_opacity: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayClickAction {
    TogglePause,
    OpenMenu,
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

pub const MAX_CAPTURE_MICROPHONES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MicrophoneSlot {
    Primary,
    Secondary,
}

impl MicrophoneSlot {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Secondary => "secondary",
        }
    }

    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Primary),
            1 => Some(Self::Secondary),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureMicrophoneEntry {
    pub id: String,
    pub label: String,
}

/// Per-source override for "auto-prompt on detection". Only recognized source
/// ids (see [`crate::sources`]) are kept; entries with an unknown id are dropped
/// during [`AppSettings::apply_patch`]. Sources without an entry fall back to the
/// source's `default_auto_prompt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceAutoPromptEntry {
    pub source_id: String,
    pub auto_prompt: bool,
}

/// User-defined system-audio source. WakeNote matches the active window title
/// or owning app name against `title_patterns` and captures the owning app
/// process when matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomSourceEntry {
    pub id: String,
    pub label: String,
    pub title_patterns: Vec<String>,
    pub auto_prompt: bool,
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
    #[serde(default)]
    pub capture_microphones: Vec<CaptureMicrophoneEntry>,
    #[serde(default = "default_merge_microphone_inputs")]
    pub merge_microphone_inputs: bool,
    pub save_root: String,
    pub save_root_confirmed: bool,
    pub audio_format: AudioFormat,
    #[serde(default = "default_audio_bitrate_kbps")]
    pub audio_bitrate_kbps: u32,
    #[serde(default = "default_mic_input_volume_percent")]
    pub mic_input_volume_percent: u32,
    pub threshold_dbfs: f32,
    pub calibration_completed: bool,
    pub attack_ms: u64,
    pub release_ms: u64,
    pub pre_roll_ms: u64,
    #[serde(default = "default_lead_in_padding_ms")]
    pub lead_in_padding_ms: u64,
    pub post_roll_ms: u64,
    pub min_chunk_ms: u64,
    pub max_chunk_ms: u64,
    pub selected_model: String,
    pub model_directory: String,
    pub vad_enabled: bool,
    pub launch_at_login: bool,
    pub start_live_input_on_launch: bool,
    #[serde(default)]
    pub input_monitoring_enabled: bool,
    #[serde(default)]
    pub auto_transcript_input_enabled: bool,
    #[serde(default)]
    pub auto_transcript_input_trailing_space: bool,
    pub show_dock_icon: bool,
    pub show_tray_icon: bool,
    #[serde(default = "default_tray_left_click_action")]
    pub tray_left_click_action: TrayClickAction,
    pub show_floating_overlay: bool,
    pub floating_overlay_position: FloatingOverlayPosition,
    #[serde(default = "default_floating_overlay_font_size_px")]
    pub floating_overlay_font_size_px: u32,
    #[serde(default = "default_floating_overlay_text_color")]
    pub floating_overlay_text_color: String,
    #[serde(default = "default_floating_overlay_background_color")]
    pub floating_overlay_background_color: String,
    #[serde(default = "default_floating_overlay_background_opacity")]
    pub floating_overlay_background_opacity: u8,
    pub theme_mode: ThemeMode,
    pub theme_primary_color: String,
    /// Master switch for system-audio (Google Meet / YouTube …) capture.
    #[serde(default)]
    pub system_audio_enabled: bool,
    /// When on, the Transcripts player auto-advances to the next item on end.
    #[serde(default)]
    pub autoplay_next_transcript: bool,
    /// OpenRouter model used for transcript summary/report generation.
    #[serde(default = "default_openrouter_model")]
    pub openrouter_model: String,
    /// Prompt template for concise transcript summaries.
    #[serde(default = "default_llm_summary_prompt_template")]
    pub llm_summary_prompt_template: String,
    /// Prompt template for detailed transcript reports.
    #[serde(default = "default_llm_report_prompt_template")]
    pub llm_report_prompt_template: String,
    /// Maximum refinement iterations for LLM-generated output.
    #[serde(default = "default_llm_max_iterations")]
    pub llm_max_iterations: u8,
    /// Per-source "auto-prompt on detection" overrides; see [`resolve_auto_prompt`].
    #[serde(default)]
    pub source_auto_prompt: Vec<SourceAutoPromptEntry>,
    /// User-defined system-audio sources matched by window title or app name.
    #[serde(default)]
    pub custom_sources: Vec<CustomSourceEntry>,
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
    pub capture_microphones: Option<Vec<CaptureMicrophoneEntry>>,
    pub merge_microphone_inputs: Option<bool>,
    pub save_root: Option<String>,
    pub audio_format: Option<AudioFormat>,
    pub audio_bitrate_kbps: Option<u32>,
    pub mic_input_volume_percent: Option<u32>,
    pub threshold_dbfs: Option<f32>,
    pub calibration_completed: Option<bool>,
    pub attack_ms: Option<u64>,
    pub release_ms: Option<u64>,
    pub pre_roll_ms: Option<u64>,
    pub lead_in_padding_ms: Option<u64>,
    pub post_roll_ms: Option<u64>,
    pub min_chunk_ms: Option<u64>,
    pub max_chunk_ms: Option<u64>,
    pub selected_model: Option<String>,
    pub model_directory: Option<String>,
    pub vad_enabled: Option<bool>,
    pub launch_at_login: Option<bool>,
    pub start_live_input_on_launch: Option<bool>,
    pub input_monitoring_enabled: Option<bool>,
    pub auto_transcript_input_enabled: Option<bool>,
    pub auto_transcript_input_trailing_space: Option<bool>,
    pub show_dock_icon: Option<bool>,
    pub show_tray_icon: Option<bool>,
    pub tray_left_click_action: Option<TrayClickAction>,
    pub show_floating_overlay: Option<bool>,
    pub floating_overlay_position: Option<FloatingOverlayPosition>,
    pub floating_overlay_font_size_px: Option<u32>,
    pub floating_overlay_text_color: Option<String>,
    pub floating_overlay_background_color: Option<String>,
    pub floating_overlay_background_opacity: Option<u8>,
    pub theme_mode: Option<ThemeMode>,
    pub theme_primary_color: Option<String>,
    pub system_audio_enabled: Option<bool>,
    pub autoplay_next_transcript: Option<bool>,
    pub openrouter_model: Option<String>,
    pub llm_summary_prompt_template: Option<String>,
    pub llm_report_prompt_template: Option<String>,
    pub llm_max_iterations: Option<u8>,
    pub source_auto_prompt: Option<Vec<SourceAutoPromptEntry>>,
    pub custom_sources: Option<Vec<CustomSourceEntry>>,
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
    Reconcile,
    Unchanged,
}

pub fn default_microphone_priority() -> Vec<MicrophonePriorityEntry> {
    vec![MicrophonePriorityEntry {
        id: "default".to_string(),
        label: "System Default".to_string(),
    }]
}

pub fn default_capture_microphones() -> Vec<CaptureMicrophoneEntry> {
    vec![CaptureMicrophoneEntry {
        id: "default".to_string(),
        label: "System Default".to_string(),
    }]
}

pub const fn default_merge_microphone_inputs() -> bool {
    true
}

pub fn normalize_capture_microphones(
    entries: Vec<CaptureMicrophoneEntry>,
) -> Vec<CaptureMicrophoneEntry> {
    let mut seen = std::collections::HashSet::new();
    let mut normalized = entries
        .into_iter()
        .filter_map(|entry| {
            let id = entry.id.trim().to_string();
            if id.is_empty() || !seen.insert(id.clone()) {
                return None;
            }
            let label = if entry.label.trim().is_empty() {
                if id == "default" {
                    "System Default".to_string()
                } else {
                    id.clone()
                }
            } else {
                entry.label.trim().to_string()
            };
            Some(CaptureMicrophoneEntry { id, label })
        })
        .take(MAX_CAPTURE_MICROPHONES)
        .collect::<Vec<_>>();

    if normalized.len() > 1 && normalized.iter().any(|entry| entry.id == "default") {
        normalized.truncate(1);
    }
    if normalized.is_empty() {
        return default_capture_microphones();
    }
    normalized
}

pub fn default_audio_bitrate_kbps() -> u32 {
    96
}

pub fn default_lead_in_padding_ms() -> u64 {
    200
}

pub fn default_tray_left_click_action() -> TrayClickAction {
    TrayClickAction::OpenMenu
}

pub fn clamp_audio_bitrate_kbps(value: u32) -> u32 {
    match value {
        0..=80 => 64,
        81..=112 => 96,
        _ => 128,
    }
}

pub fn clamp_mic_input_volume_percent(value: u32) -> u32 {
    value.clamp(MIC_INPUT_VOLUME_MIN_PERCENT, MIC_INPUT_VOLUME_MAX_PERCENT)
}

pub fn clamp_llm_max_iterations(value: u8) -> u8 {
    value.clamp(LLM_MAX_ITERATIONS_MIN, LLM_MAX_ITERATIONS_MAX)
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
    let capture_microphones_changed = patch.capture_microphones.as_ref().is_some_and(|entries| {
        normalize_capture_microphones(entries.clone()) != settings.capture_microphones
    });
    let merge_microphone_inputs_changed = patch
        .merge_microphone_inputs
        .is_some_and(|value| value != settings.merge_microphone_inputs);
    let microphone_changed = patch
        .selected_microphone
        .as_ref()
        .is_some_and(|value| value != &settings.selected_microphone)
        || patch
            .microphone_priority
            .as_ref()
            .is_some_and(|list| top_priority_id(list) != settings.selected_microphone);

    match (
        currently_running,
        should_run,
        microphone_changed,
        capture_microphones_changed,
        merge_microphone_inputs_changed,
    ) {
        (false, true, _, _, _) => LiveCaptureRuntimeAction::Start,
        (true, false, _, _, _) => LiveCaptureRuntimeAction::Stop,
        (true, true, _, true, _) | (true, true, _, _, true) => LiveCaptureRuntimeAction::Reconcile,
        (true, true, true, false, false) => LiveCaptureRuntimeAction::Restart,
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

/// Whether detection should auto-prompt for a recognized source. A user override
/// in `source_auto_prompt` wins; otherwise the source's `default_auto_prompt`.
/// Unknown source ids return `false`.
pub fn resolve_auto_prompt(settings: &AppSettings, source_id: &str) -> bool {
    if let Some(entry) = settings
        .source_auto_prompt
        .iter()
        .find(|entry| entry.source_id == source_id)
    {
        return entry.auto_prompt;
    }
    crate::sources::recognized_source(source_id)
        .map(|source| source.default_auto_prompt)
        .or_else(|| {
            settings
                .custom_sources
                .iter()
                .find(|source| source.id == source_id)
                .map(|source| source.auto_prompt)
        })
        .unwrap_or(false)
}

fn source_id_is_known(settings: &AppSettings, source_id: &str) -> bool {
    crate::sources::recognized_source(source_id).is_some()
        || settings
            .custom_sources
            .iter()
            .any(|source| source.id == source_id)
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

fn slugify_source_id(value: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_separator = false;
        } else if (ch.is_ascii_whitespace() || ch == '-' || ch == '_') && !last_was_separator {
            slug.push('-');
            last_was_separator = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn normalize_source_id(
    requested_id: &str,
    label: &str,
    index: usize,
    used: &mut std::collections::HashSet<String>,
) -> String {
    let mut base = slugify_source_id(requested_id);
    if base.is_empty() {
        base = slugify_source_id(label);
    }
    if base.is_empty() {
        base = format!("source-{}", index + 1);
    }
    if crate::sources::recognized_source(&base).is_some() {
        base = format!("custom-{base}");
    }

    let mut candidate = base.clone();
    let mut suffix = 2;
    while used.contains(&candidate) || crate::sources::recognized_source(&candidate).is_some() {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    used.insert(candidate.clone());
    candidate
}

fn normalize_custom_sources(list: Vec<CustomSourceEntry>) -> Vec<CustomSourceEntry> {
    let mut used_ids = std::collections::HashSet::new();
    list.into_iter()
        .enumerate()
        .filter_map(|(index, source)| {
            let label = source.label.trim().to_string();
            if label.is_empty() {
                return None;
            }

            let mut seen_patterns = std::collections::HashSet::new();
            let title_patterns = source
                .title_patterns
                .into_iter()
                .filter_map(|pattern| {
                    let trimmed = pattern.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    let key = trimmed.to_lowercase();
                    if !seen_patterns.insert(key) {
                        return None;
                    }
                    Some(trimmed.to_string())
                })
                .collect::<Vec<_>>();
            if title_patterns.is_empty() {
                return None;
            }

            Some(CustomSourceEntry {
                id: normalize_source_id(&source.id, &label, index, &mut used_ids),
                label,
                title_patterns,
                auto_prompt: source.auto_prompt,
            })
        })
        .collect()
}

impl AppSettings {
    pub fn normalize_capture_microphones(&mut self) {
        if self.capture_microphones.is_empty() {
            self.capture_microphones = vec![CaptureMicrophoneEntry {
                id: if self.selected_microphone.is_empty() {
                    "default".to_string()
                } else {
                    self.selected_microphone.clone()
                },
                label: if self.selected_microphone_label.is_empty() {
                    if self.selected_microphone == "default" {
                        "System Default".to_string()
                    } else {
                        self.selected_microphone.clone()
                    }
                } else {
                    self.selected_microphone_label.clone()
                },
            }];
        }
        self.capture_microphones =
            normalize_capture_microphones(std::mem::take(&mut self.capture_microphones));
        let primary = self
            .capture_microphones
            .first()
            .cloned()
            .expect("capture microphones are non-empty after normalization");
        self.selected_microphone = primary.id.clone();
        self.selected_microphone_label = primary.label.clone();

        if let Some(existing) = self.microphone_priority.first_mut() {
            existing.id = primary.id;
            existing.label = primary.label;
        } else {
            self.microphone_priority.push(MicrophonePriorityEntry {
                id: primary.id,
                label: primary.label,
            });
        }
    }

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
        let capture_microphones_patch = patch.capture_microphones.clone();
        let legacy_microphone_patch = patch.selected_microphone.is_some()
            || patch.selected_microphone_label.is_some()
            || patch.microphone_priority.is_some();
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
        if let Some(list) = capture_microphones_patch {
            self.capture_microphones = list;
        } else if legacy_microphone_patch {
            self.capture_microphones = vec![CaptureMicrophoneEntry {
                id: self.selected_microphone.clone(),
                label: self.selected_microphone_label.clone(),
            }];
        }
        self.normalize_capture_microphones();
        if let Some(value) = patch.merge_microphone_inputs {
            self.merge_microphone_inputs = value;
        }
        if let Some(value) = patch.save_root {
            self.save_root_confirmed = !value.trim().is_empty();
            self.save_root = value;
        }
        if let Some(value) = patch.audio_format {
            self.audio_format = value;
        }
        if let Some(value) = patch.audio_bitrate_kbps {
            self.audio_bitrate_kbps = clamp_audio_bitrate_kbps(value);
        }
        if let Some(value) = patch.mic_input_volume_percent {
            self.mic_input_volume_percent = clamp_mic_input_volume_percent(value);
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
        if let Some(value) = patch.lead_in_padding_ms {
            self.lead_in_padding_ms = clamp_ms(value, 0, 2_000);
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
        if let Some(value) = patch.input_monitoring_enabled {
            self.input_monitoring_enabled = value;
        }
        if let Some(value) = patch.auto_transcript_input_enabled {
            self.auto_transcript_input_enabled = value;
        }
        if let Some(value) = patch.auto_transcript_input_trailing_space {
            self.auto_transcript_input_trailing_space = value;
        }
        if let Some(value) = patch.show_dock_icon {
            self.show_dock_icon = value;
        }
        if let Some(value) = patch.show_tray_icon {
            self.show_tray_icon = value;
        }
        if let Some(value) = patch.tray_left_click_action {
            self.tray_left_click_action = value;
        }
        if let Some(value) = patch.show_floating_overlay {
            self.show_floating_overlay = value;
        }
        if let Some(value) = patch.floating_overlay_position {
            self.floating_overlay_position = value;
        }
        if let Some(value) = patch.floating_overlay_font_size_px {
            self.floating_overlay_font_size_px = value.clamp(
                FLOATING_OVERLAY_FONT_SIZE_MIN_PX,
                FLOATING_OVERLAY_FONT_SIZE_MAX_PX,
            );
        }
        if let Some(value) = patch.floating_overlay_text_color {
            self.floating_overlay_text_color = value;
        }
        if let Some(value) = patch.floating_overlay_background_color {
            self.floating_overlay_background_color = value;
        }
        if let Some(value) = patch.floating_overlay_background_opacity {
            self.floating_overlay_background_opacity = value.clamp(
                FLOATING_OVERLAY_BACKGROUND_OPACITY_MIN,
                FLOATING_OVERLAY_BACKGROUND_OPACITY_MAX,
            );
        }
        if let Some(value) = patch.theme_mode {
            self.theme_mode = value;
        }
        if let Some(value) = patch.theme_primary_color {
            self.theme_primary_color = value;
        }
        if let Some(value) = patch.system_audio_enabled {
            self.system_audio_enabled = value;
        }
        if let Some(value) = patch.autoplay_next_transcript {
            self.autoplay_next_transcript = value;
        }
        if let Some(value) = patch.openrouter_model {
            self.openrouter_model = if value.trim().is_empty() {
                default_openrouter_model()
            } else {
                value.trim().to_string()
            };
        }
        if let Some(value) = patch.llm_summary_prompt_template {
            self.llm_summary_prompt_template = if value.trim().is_empty() {
                default_llm_summary_prompt_template()
            } else {
                value
            };
        }
        if let Some(value) = patch.llm_report_prompt_template {
            self.llm_report_prompt_template = if value.trim().is_empty() {
                default_llm_report_prompt_template()
            } else {
                value
            };
        }
        if let Some(value) = patch.llm_max_iterations {
            self.llm_max_iterations = clamp_llm_max_iterations(value);
        }
        if let Some(list) = patch.custom_sources {
            self.custom_sources = normalize_custom_sources(list);
        }
        if let Some(list) = patch.source_auto_prompt {
            // Normalize against the known source list: drop unknown ids.
            self.source_auto_prompt = list
                .into_iter()
                .filter(|entry| source_id_is_known(self, &entry.source_id))
                .collect();
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
            capture_microphones: default_capture_microphones(),
            merge_microphone_inputs: default_merge_microphone_inputs(),
            save_root: "~/Documents/WakeNote".to_string(),
            save_root_confirmed: false,
            audio_format: AudioFormat::M4a,
            audio_bitrate_kbps: default_audio_bitrate_kbps(),
            mic_input_volume_percent: default_mic_input_volume_percent(),
            threshold_dbfs: -40.0,
            calibration_completed: false,
            attack_ms: 200,
            release_ms: 1_000,
            pre_roll_ms: 400,
            lead_in_padding_ms: default_lead_in_padding_ms(),
            post_roll_ms: 400,
            min_chunk_ms: 800,
            max_chunk_ms: 180_000,
            selected_model: "whisper-medium".to_string(),
            model_directory: "~/Library/Application Support/WakeNote/models".to_string(),
            vad_enabled: false,
            launch_at_login: false,
            start_live_input_on_launch: true,
            input_monitoring_enabled: false,
            auto_transcript_input_enabled: false,
            auto_transcript_input_trailing_space: false,
            show_dock_icon: true,
            show_tray_icon: true,
            tray_left_click_action: default_tray_left_click_action(),
            show_floating_overlay: true,
            floating_overlay_position: FloatingOverlayPosition::Top,
            floating_overlay_font_size_px: default_floating_overlay_font_size_px(),
            floating_overlay_text_color: default_floating_overlay_text_color(),
            floating_overlay_background_color: default_floating_overlay_background_color(),
            floating_overlay_background_opacity: default_floating_overlay_background_opacity(),
            theme_mode: ThemeMode::Dark,
            theme_primary_color: "#000".to_string(),
            system_audio_enabled: false,
            autoplay_next_transcript: false,
            openrouter_model: default_openrouter_model(),
            llm_summary_prompt_template: default_llm_summary_prompt_template(),
            llm_report_prompt_template: default_llm_report_prompt_template(),
            llm_max_iterations: default_llm_max_iterations(),
            source_auto_prompt: Vec::new(),
            custom_sources: Vec::new(),
        }
    }
}

impl AppSettings {
    pub fn tray_right_click_action(&self) -> TrayClickAction {
        match self.tray_left_click_action {
            TrayClickAction::TogglePause => TrayClickAction::OpenMenu,
            TrayClickAction::OpenMenu => TrayClickAction::TogglePause,
        }
    }

    pub fn effective_floating_overlay_position(&self) -> FloatingOverlayPosition {
        if self.show_floating_overlay {
            self.floating_overlay_position
        } else {
            FloatingOverlayPosition::Off
        }
    }

    pub fn floating_overlay_caption_style(&self) -> FloatingOverlayCaptionStyle {
        FloatingOverlayCaptionStyle {
            font_size_px: self.floating_overlay_font_size_px,
            text_color: self.floating_overlay_text_color.clone(),
            background_color: self.floating_overlay_background_color.clone(),
            background_opacity: self.floating_overlay_background_opacity,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_sets_system_audio_enabled() {
        let mut settings = AppSettings::default();
        assert!(!settings.system_audio_enabled);
        settings.apply_patch(SettingsPatch {
            system_audio_enabled: Some(true),
            ..Default::default()
        });
        assert!(settings.system_audio_enabled);
    }

    #[test]
    fn patch_sets_input_monitoring_without_restarting_live_capture() {
        let mut settings = AppSettings::default();
        assert!(!settings.input_monitoring_enabled);

        let action = live_capture_runtime_action_for_patch(
            &settings,
            &SettingsPatch {
                input_monitoring_enabled: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(action, LiveCaptureRuntimeAction::Unchanged);

        settings.apply_patch(SettingsPatch {
            input_monitoring_enabled: Some(true),
            ..Default::default()
        });
        assert!(settings.input_monitoring_enabled);
    }

    #[test]
    fn patch_sets_autoplay_next_transcript() {
        let mut settings = AppSettings::default();
        assert!(!settings.autoplay_next_transcript);
        settings.apply_patch(SettingsPatch {
            autoplay_next_transcript: Some(true),
            ..Default::default()
        });
        assert!(settings.autoplay_next_transcript);
    }

    #[test]
    fn llm_defaults_use_zai_glm_52_and_three_iterations() {
        let settings = AppSettings::default();

        assert_eq!(settings.openrouter_model, "z-ai/glm-5.2");
        assert_eq!(settings.llm_max_iterations, 3);
        assert!(
            settings
                .llm_summary_prompt_template
                .contains("{{transcripts}}")
        );
        assert!(
            settings
                .llm_report_prompt_template
                .contains("# Action Items")
        );
    }

    #[test]
    fn patch_clamps_llm_iterations_to_supported_range() {
        let mut settings = AppSettings::default();

        settings.apply_patch(SettingsPatch {
            llm_max_iterations: Some(0),
            ..Default::default()
        });
        assert_eq!(settings.llm_max_iterations, 1);

        settings.apply_patch(SettingsPatch {
            llm_max_iterations: Some(30),
            ..Default::default()
        });
        assert_eq!(settings.llm_max_iterations, 30);

        settings.apply_patch(SettingsPatch {
            llm_max_iterations: Some(99),
            ..Default::default()
        });
        assert_eq!(settings.llm_max_iterations, 30);
    }

    #[test]
    fn patch_trims_openrouter_model_and_restores_blank_templates() {
        let mut settings = AppSettings::default();

        settings.apply_patch(SettingsPatch {
            openrouter_model: Some("  z-ai/glm-5.2  ".into()),
            llm_summary_prompt_template: Some("custom {{transcripts}}".into()),
            llm_report_prompt_template: Some(" ".into()),
            ..Default::default()
        });

        assert_eq!(settings.openrouter_model, "z-ai/glm-5.2");
        assert_eq!(
            settings.llm_summary_prompt_template,
            "custom {{transcripts}}"
        );
        assert_eq!(
            settings.llm_report_prompt_template,
            default_llm_report_prompt_template()
        );
    }

    #[test]
    fn default_tray_left_click_opens_menu_and_right_click_toggles_app() {
        let settings = AppSettings::default();

        assert_eq!(settings.tray_left_click_action, TrayClickAction::OpenMenu);
        assert_eq!(
            settings.tray_right_click_action(),
            TrayClickAction::TogglePause
        );
    }

    #[test]
    fn patch_can_swap_tray_click_actions() {
        let mut settings = AppSettings::default();
        settings.apply_patch(SettingsPatch {
            tray_left_click_action: Some(TrayClickAction::OpenMenu),
            ..Default::default()
        });

        assert_eq!(settings.tray_left_click_action, TrayClickAction::OpenMenu);
        assert_eq!(
            settings.tray_right_click_action(),
            TrayClickAction::TogglePause
        );
    }

    #[test]
    fn patch_sets_automatic_transcript_input_mode() {
        let mut settings = AppSettings::default();
        assert!(!settings.auto_transcript_input_enabled);
        assert!(!settings.auto_transcript_input_trailing_space);

        settings.apply_patch(SettingsPatch {
            auto_transcript_input_enabled: Some(true),
            auto_transcript_input_trailing_space: Some(true),
            ..Default::default()
        });

        assert!(settings.auto_transcript_input_enabled);
        assert!(settings.auto_transcript_input_trailing_space);
    }

    #[test]
    fn default_audio_bitrate_is_96_kbps() {
        assert_eq!(AppSettings::default().audio_bitrate_kbps, 96);
    }

    #[test]
    fn default_mic_input_volume_is_neutral() {
        assert_eq!(AppSettings::default().mic_input_volume_percent, 100);
    }

    #[test]
    fn patch_clamps_mic_input_volume() {
        let mut settings = AppSettings::default();
        settings.apply_patch(SettingsPatch {
            mic_input_volume_percent: Some(250),
            ..Default::default()
        });
        assert_eq!(settings.mic_input_volume_percent, 200);

        settings.apply_patch(SettingsPatch {
            mic_input_volume_percent: Some(0),
            ..Default::default()
        });
        assert_eq!(settings.mic_input_volume_percent, 0);
    }

    #[test]
    fn default_vad_timing_uses_tuned_capture_profile() {
        let settings = AppSettings::default();
        assert_eq!(settings.threshold_dbfs, -40.0);
        assert_eq!(settings.mic_input_volume_percent, 100);
        assert_eq!(settings.attack_ms, 200);
        assert_eq!(settings.release_ms, 1_000);
        assert_eq!(settings.pre_roll_ms, 400);
        assert_eq!(settings.lead_in_padding_ms, 200);
        assert_eq!(settings.post_roll_ms, 400);
        assert_eq!(settings.min_chunk_ms, 800);
        assert_eq!(settings.max_chunk_ms, 180_000);
    }

    #[test]
    fn patch_clamps_lead_in_padding() {
        let mut settings = AppSettings::default();
        settings.apply_patch(SettingsPatch {
            lead_in_padding_ms: Some(10_000),
            ..Default::default()
        });
        assert_eq!(settings.lead_in_padding_ms, 2_000);
    }

    #[test]
    fn patch_snaps_audio_bitrate_to_supported_presets() {
        let mut settings = AppSettings::default();
        settings.apply_patch(SettingsPatch {
            audio_bitrate_kbps: Some(128),
            ..Default::default()
        });
        assert_eq!(settings.audio_bitrate_kbps, 128);

        settings.apply_patch(SettingsPatch {
            audio_bitrate_kbps: Some(95),
            ..Default::default()
        });
        assert_eq!(settings.audio_bitrate_kbps, 96);

        settings.apply_patch(SettingsPatch {
            audio_bitrate_kbps: Some(1),
            ..Default::default()
        });
        assert_eq!(settings.audio_bitrate_kbps, 64);
    }

    #[test]
    fn source_auto_prompt_override_kept_and_unknown_dropped() {
        let mut settings = AppSettings::default();
        settings.apply_patch(SettingsPatch {
            source_auto_prompt: Some(vec![
                SourceAutoPromptEntry {
                    source_id: "youtube".into(),
                    auto_prompt: true,
                },
                SourceAutoPromptEntry {
                    source_id: "zoom".into(),
                    auto_prompt: true,
                },
                SourceAutoPromptEntry {
                    source_id: "unknown".into(),
                    auto_prompt: true,
                },
            ]),
            ..Default::default()
        });
        assert_eq!(settings.source_auto_prompt.len(), 2);
        assert!(resolve_auto_prompt(&settings, "youtube"));
        assert!(resolve_auto_prompt(&settings, "zoom"));
        assert!(!resolve_auto_prompt(&settings, "unknown"));
    }

    #[test]
    fn patch_keeps_valid_custom_sources_and_drops_empty_entries() {
        let mut settings = AppSettings::default();
        settings.apply_patch(SettingsPatch {
            custom_sources: Some(vec![
                CustomSourceEntry {
                    id: "zoom".into(),
                    label: " Zoom ".into(),
                    title_patterns: vec!["Zoom Meeting".into(), "".into(), " zoom meeting ".into()],
                    auto_prompt: true,
                },
                CustomSourceEntry {
                    id: "empty".into(),
                    label: " ".into(),
                    title_patterns: vec!["".into()],
                    auto_prompt: true,
                },
            ]),
            ..Default::default()
        });

        assert_eq!(
            settings.custom_sources,
            vec![CustomSourceEntry {
                id: "custom-zoom".into(),
                label: "Zoom".into(),
                title_patterns: vec!["Zoom Meeting".into()],
                auto_prompt: true,
            }]
        );
        assert!(resolve_auto_prompt(&settings, "zoom"));
    }

    #[test]
    fn resolve_auto_prompt_uses_source_defaults() {
        let settings = AppSettings::default();
        assert!(resolve_auto_prompt(&settings, "meet"));
        assert!(resolve_auto_prompt(&settings, "youtube"));
        assert!(!resolve_auto_prompt(&settings, "unknown"));
    }

    #[test]
    fn legacy_settings_without_new_fields_deserialize() {
        let json = r##"{
            "recording_enabled": true, "transcription_enabled": true,
            "transcription_language": "ko", "suppress_low_confidence_transcripts": true,
            "pause_all": false, "selected_microphone": "default",
            "selected_microphone_label": "System Default", "save_root": "~/Documents/WakeNote",
            "save_root_confirmed": false, "audio_format": "m4a", "threshold_dbfs": -42.0,
            "calibration_completed": false, "attack_ms": 300, "release_ms": 1000,
            "pre_roll_ms": 600, "post_roll_ms": 300, "min_chunk_ms": 600, "max_chunk_ms": 120000,
            "selected_model": "whisper-medium",
            "model_directory": "~/Library/Application Support/WakeNote/models",
            "vad_enabled": false, "launch_at_login": false, "start_live_input_on_launch": true,
            "input_monitoring_enabled": false,
            "show_dock_icon": true, "show_tray_icon": true, "show_floating_overlay": true,
            "floating_overlay_position": "top", "theme_mode": "dark", "theme_primary_color": "#000"
        }"##;
        let settings: AppSettings =
            serde_json::from_str(json).expect("legacy settings deserialize");
        assert_eq!(settings.audio_bitrate_kbps, 96);
        assert_eq!(settings.mic_input_volume_percent, 100);
        assert_eq!(settings.lead_in_padding_ms, 200);
        assert!(!settings.system_audio_enabled);
        assert!(!settings.auto_transcript_input_trailing_space);
        assert!(settings.source_auto_prompt.is_empty());
        assert!(settings.custom_sources.is_empty());
        assert_eq!(settings.openrouter_model, OPENROUTER_DEFAULT_MODEL_ID);
        assert_eq!(settings.llm_max_iterations, 3);
        assert_eq!(
            settings.llm_summary_prompt_template,
            default_llm_summary_prompt_template()
        );
        assert_eq!(
            settings.llm_report_prompt_template,
            default_llm_report_prompt_template()
        );
        assert_eq!(settings.floating_overlay_font_size_px, 24);
        assert_eq!(settings.floating_overlay_text_color, "#ffffff");
        assert_eq!(settings.floating_overlay_background_color, "#050507");
        assert_eq!(settings.floating_overlay_background_opacity, 82);
    }

    #[test]
    fn patch_clamps_floating_overlay_caption_style() {
        let mut settings = AppSettings::default();
        settings.apply_patch(SettingsPatch {
            floating_overlay_font_size_px: Some(4),
            floating_overlay_text_color: Some("#f8fafc".into()),
            floating_overlay_background_color: Some("#123456".into()),
            floating_overlay_background_opacity: Some(128),
            ..Default::default()
        });

        assert_eq!(settings.floating_overlay_font_size_px, 18);
        assert_eq!(settings.floating_overlay_text_color, "#f8fafc");
        assert_eq!(settings.floating_overlay_background_color, "#123456");
        assert_eq!(settings.floating_overlay_background_opacity, 100);
    }
}
