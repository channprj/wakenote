export type AudioFormat = "m4a" | "wav";

export type FloatingOverlayPosition = "off" | "top" | "bottom";
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

export interface AppSettings {
  recording_enabled: boolean;
  transcription_enabled: boolean;
  transcription_language: TranscriptionLanguage;
  suppress_low_confidence_transcripts: boolean;
  pause_all: boolean;
  selected_microphone: string;
  selected_microphone_label: string;
  save_root: string;
  save_root_confirmed: boolean;
  audio_format: AudioFormat;
  threshold_dbfs: number;
  calibration_completed: boolean;
  attack_ms: number;
  release_ms: number;
  pre_roll_ms: number;
  post_roll_ms: number;
  min_chunk_ms: number;
  max_chunk_ms: number;
  selected_model: string;
  model_directory: string;
  vad_enabled: boolean;
  launch_at_login: boolean;
  start_live_input_on_launch: boolean;
  show_dock_icon: boolean;
  show_tray_icon: boolean;
  show_floating_overlay: boolean;
  floating_overlay_position: FloatingOverlayPosition;
  theme_primary_color: string;
}

export type SettingsPatch = Partial<AppSettings>;

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
}

export interface LevelSnapshot {
  current_dbfs: number;
  peak_dbfs: number;
  noise_floor_dbfs: number;
  suggested_threshold_dbfs: number;
}

export interface AppStatus {
  mode: AppMode;
  tray_state: TrayState;
  live_input_active: boolean;
  active_model: string;
  active_microphone: string;
  microphone_warning?: string | null;
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
}
