export type AudioFormat = "m4a" | "wav";

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
  pause_all: boolean;
  selected_microphone: string;
  selected_microphone_label: string;
  save_root: string;
  audio_format: AudioFormat;
  threshold_dbfs: number;
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
  show_tray_icon: boolean;
  show_floating_overlay: boolean;
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

export interface AppStatus {
  mode: AppMode;
  tray_state: TrayState;
  active_model: string;
  active_microphone: string;
  threshold_dbfs: number;
  queue: QueueSnapshot;
}

export interface AppSnapshot {
  settings: AppSettings;
  status: AppStatus;
  microphones: MicrophoneDevice[];
  models: ModelDescriptor[];
  queue: QueueSnapshot;
}
