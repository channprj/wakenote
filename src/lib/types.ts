export type AudioFormat = "m4a" | "mp3" | "wav";

export type FloatingOverlayPosition = "off" | "top" | "bottom";
export type ThemeMode = "light" | "dark";
export type TrayClickAction = "toggle_pause" | "open_menu";
export type TranscriptionLanguage =
  | "auto"
  | "ko"
  | "en"
  | "ja"
  | "zh"
  | "es"
  | "fr"
  | "de";

export type AppMode =
  | "recording_and_transcription"
  | "recording_only"
  | "transcription_only"
  | "paused";

export type TrayState =
  | "idle"
  | "listening"
  | "recording"
  | "transcribing"
  | "paused"
  | "error";

export type ModelStatus =
  | "installed"
  | "missing"
  | "downloading"
  | "verifying"
  | "extracting"
  | "ready"
  | "unloaded"
  | "error";

export type QueueJobStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "cancelled"
  | "skipped";

export interface MicrophonePriorityEntry {
  id: string;
  label: string;
}

export interface AppSettings {
  recording_enabled: boolean;
  transcription_enabled: boolean;
  transcription_language: TranscriptionLanguage;
  suppress_low_confidence_transcripts: boolean;
  pause_all: boolean;
  selected_microphone: string;
  selected_microphone_label: string;
  microphone_priority: MicrophonePriorityEntry[];
  save_root: string;
  save_root_confirmed: boolean;
  audio_format: AudioFormat;
  audio_bitrate_kbps: number;
  threshold_dbfs: number;
  calibration_completed: boolean;
  attack_ms: number;
  release_ms: number;
  pre_roll_ms: number;
  lead_in_padding_ms: number;
  post_roll_ms: number;
  min_chunk_ms: number;
  max_chunk_ms: number;
  selected_model: string;
  model_directory: string;
  vad_enabled: boolean;
  launch_at_login: boolean;
  start_live_input_on_launch: boolean;
  auto_transcript_input_enabled: boolean;
  auto_transcript_input_trailing_space: boolean;
  show_dock_icon: boolean;
  show_tray_icon: boolean;
  tray_left_click_action: TrayClickAction;
  show_floating_overlay: boolean;
  floating_overlay_position: FloatingOverlayPosition;
  theme_mode: ThemeMode;
  theme_primary_color: string;
  system_audio_enabled: boolean;
  source_auto_prompt: SourceAutoPromptEntry[];
  custom_sources: CustomSourceEntry[];
}

export type SettingsPatch = Partial<AppSettings>;

/** Audio source a recorded chunk came from. Mirrors the Rust `ChunkSource`. */
export type ChunkSource = "microphone" | "system";

/** Per-source "auto-prompt on detection" override. */
export interface SourceAutoPromptEntry {
  source_id: string;
  auto_prompt: boolean;
}

/** User-defined system-audio source matched by window title or app name. */
export interface CustomSourceEntry {
  id: string;
  label: string;
  title_patterns: string[];
  auto_prompt: boolean;
}

/** A recognized capture source with its resolved auto-capture setting. */
export interface RecognizedSourceInfo {
  id: string;
  label: string;
  description: string;
  auto_prompt: boolean;
  title_patterns: string[];
  custom: boolean;
}

/** A recognized source currently detected on screen (source-detected/ended). */
export interface SourcePayload {
  source_id: string;
  label: string;
  app_name: string;
}

/** Current system-audio detection/capture state (source_capture_status). */
export interface SourceCaptureStatus {
  detected: SourcePayload | null;
  capturing: boolean;
}

export interface MicrophoneDevice {
  id: string;
  label: string;
  available: boolean;
  fallback: boolean;
}

export interface ModelDescriptor {
  id: string;
  display_name: string;
  engine: string;
  provider_runtime: string;
  download_url?: string | null;
  checksum_sha256?: string | null;
  size_mb: number;
  languages: string[];
  speed_score: number;
  accuracy_score: number;
  offline: boolean;
  status: ModelStatus;
  download_progress?: number | null;
  download_error?: string | null;
}

export interface QueueJob {
  id: number;
  audio_path: string;
  model_id: string;
  status: QueueJobStatus;
  error?: string | null;
}

export interface QueueSnapshot {
  jobs: QueueJob[];
  pending_count: number;
  running_count: number;
  failed_count: number;
}

export interface RecentTranscript {
  transcript_path: string;
  audio_path: string | null;
  recorded_at: string;
  text: string;
  source?: ChunkSource;
  source_label?: string | null;
}

export interface TranscriptDay {
  day: string;
  count: number;
}

export interface UploadedAudio {
  audio_path: string;
  original_filename: string;
  stored_at: string;
}

export interface AudioRange {
  start: number;
  end: number;
}

export interface AudioWaveform {
  duration_seconds: number;
  sample_rate: number;
  /** Absolute (positive) per-bucket peaks 0..1, used for skip-silence and
   * fallback rendering when signed peaks are unavailable. */
  peaks: number[];
  /** Positive per-bucket peak 0..1. Drawn upward from the centerline. */
  peaks_max?: number[];
  /** Negative per-bucket peak -1..0. Drawn downward from the centerline. */
  peaks_min?: number[];
  audible_ranges: AudioRange[];
}

export type PermissionGrantStatus =
  | "unknown"
  | "not_determined"
  | "granted"
  | "denied"
  | "restricted"
  | "unsupported";

export interface PermissionState {
  status: PermissionGrantStatus;
  label: string;
  detail: string;
  can_request: boolean;
  can_open_settings: boolean;
}

export interface AppPermissions {
  microphone: PermissionState;
  screen_recording: PermissionState;
}

export interface LevelSnapshot {
  current_dbfs: number;
  peak_dbfs: number;
  noise_floor_dbfs: number;
  suggested_threshold_dbfs: number;
}

export interface SilenceWarning {
  device_label: string;
  seconds: number;
}

export interface AppStatus {
  mode: AppMode;
  tray_state: TrayState;
  live_input_active: boolean;
  active_model: string;
  active_microphone: string;
  microphone_warning?: string | null;
  silence_warning?: SilenceWarning | null;
  runtime_warning?: string | null;
  threshold_dbfs: number;
  level: LevelSnapshot;
  queue: QueueSnapshot;
}

export interface AppSnapshot {
  settings: AppSettings;
  status: AppStatus;
  microphones: MicrophoneDevice[];
  models: ModelDescriptor[];
  queue: QueueSnapshot;
  recent_transcripts: RecentTranscript[];
  permissions: AppPermissions;
}
