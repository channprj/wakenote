export type AudioFormat = "m4a" | "mp3" | "wav";

export type FloatingOverlayPosition = "off" | "top" | "bottom";
export type DictationCueSound = "original" | "alternative";
export type DictationCueVolume = "muted" | "small" | "medium" | "large";
export type DictationBubblePosition =
  | "top_left"
  | "top_center"
  | "top_right"
  | "bottom_left"
  | "bottom_center"
  | "bottom_right";
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

export interface CaptureMicrophoneEntry {
  id: string;
  label: string;
}

export type MicrophoneSlot = "primary" | "secondary";

export interface DictionaryEntry {
  id: string;
  term: string;
  aliases: string[];
  enabled: boolean;
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
  capture_microphones: CaptureMicrophoneEntry[];
  merge_microphone_inputs: boolean;
  save_root: string;
  save_root_confirmed: boolean;
  audio_format: AudioFormat;
  audio_bitrate_kbps: number;
  mic_input_volume_percent: number;
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
  dictionary_enabled: boolean;
  dictionary: DictionaryEntry[];
  vad_enabled: boolean;
  launch_at_login: boolean;
  start_live_input_on_launch: boolean;
  input_monitoring_enabled: boolean;
  auto_transcript_input_enabled: boolean;
  auto_transcript_input_trailing_space: boolean;
  dictation_enabled: boolean;
  dictation_shortcut: string;
  dictation_language: TranscriptionLanguage;
  dictation_start_sound: DictationCueSound;
  dictation_stop_sound: DictationCueSound;
  dictation_end_sound: DictationCueSound;
  dictation_cue_volume: DictationCueVolume;
  dictation_bubble_position: DictationBubblePosition;
  dictation_bubble_background_color: string;
  dictation_bubble_background_opacity: number;
  dictation_model: string;
  dictation_copy_to_clipboard: boolean;
  dictation_remove_trailing_space: boolean;
  show_dock_icon: boolean;
  show_tray_icon: boolean;
  tray_left_click_action: TrayClickAction;
  show_floating_overlay: boolean;
  floating_overlay_position: FloatingOverlayPosition;
  floating_overlay_font_size_px: number;
  floating_overlay_text_color: string;
  floating_overlay_background_color: string;
  floating_overlay_background_opacity: number;
  theme_mode: ThemeMode;
  theme_primary_color: string;
  system_audio_enabled: boolean;
  autoplay_next_transcript: boolean;
  openrouter_model: string;
  llm_summary_prompt_template: string;
  llm_report_prompt_template: string;
  llm_max_iterations: number;
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

export type ListVisibilityKind =
  | "meeting"
  | "transcript"
  | "report_run"
  | "legacy_report";

export interface ListVisibilityTarget {
  kind: ListVisibilityKind;
  id: string;
}

export interface SetListVisibilityRequest {
  targets: ListVisibilityTarget[];
  hidden: boolean;
}

export interface ListVisibilityState {
  meetings: string[];
  transcripts: string[];
  report_runs: string[];
  legacy_reports: string[];
}

export interface RecentTranscript {
  transcript_path: string;
  audio_path: string | null;
  recorded_at: string;
  text: string;
  source?: ChunkSource;
  source_label?: string | null;
  device_id?: string | null;
  device_name?: string | null;
  microphone_slot?: MicrophoneSlot | null;
}

export type LlmReportKind = "summary" | "detailed_report";

export type LlmCompletionReason =
  | "success_criteria_met"
  | "max_iterations_reached";

export type LlmProgressStage =
  | "preparing"
  | "generating"
  | "evaluating"
  | "refining"
  | "saving"
  | "completed"
  | "max_iterations_reached"
  | "failed"
  | "cancelled";

export interface LlmGenerateRequest {
  kind: LlmReportKind;
  transcripts: RecentTranscript[];
  run_id?: string | null;
}

export interface LlmProgressEvent {
  run_id: string;
  stage: LlmProgressStage;
  iteration: number;
  max_iterations: number;
  message: string;
  detail?: string | null;
}

export type LlmReportRunStatus =
  | "queued"
  | "running"
  | "stopping"
  | "cancelled"
  | "failed"
  | "completed";

export interface LlmReportRunSnapshot {
  run_id: string;
  parent_run_id: string | null;
  revision: number;
  status: LlmReportRunStatus;
  stage: LlmProgressStage | null;
  kind: LlmReportKind;
  created_at: string;
  updated_at: string;
  started_at: string | null;
  finished_at: string | null;
  iteration: number;
  max_iterations: number;
  message: string;
  detail: string | null;
  error: string | null;
  progress: LlmProgressEvent[];
  model: string;
  selected_count: number;
  date_range: string;
  report_id: string | null;
  report_path: string | null;
  completion_reason: LlmCompletionReason | null;
  success_criteria_met: boolean | null;
  quality_feedback: string | null;
  usage: LlmUsageTotals | null;
}

export interface LlmUsageTotals {
  request_count: number;
  prompt_tokens: number | null;
  completion_tokens: number | null;
  total_tokens: number | null;
  cost: number | null;
}

export interface LlmReportHistoryItem {
  report_id: string;
  kind: LlmReportKind;
  created_at: string;
  file_name: string;
  report_path: string;
  model: string | null;
  iterations_used: number | null;
  max_iterations: number | null;
  success_criteria_met: boolean | null;
  completion_reason: LlmCompletionReason | null;
  quality_feedback: string | null;
  selected_count: number | null;
  date_range: string | null;
  usage: LlmUsageTotals | null;
  legacy: boolean;
}

export interface LlmReportHistoryDetail {
  item: LlmReportHistoryItem;
  content: string;
}

export interface ApiKeyStatus {
  configured: boolean;
}

export type OpenRouterKeyStatus = ApiKeyStatus;

export interface TranscriptDay {
  day: string;
  count: number;
}

export interface UploadedAudio {
  audio_path: string;
  original_filename: string;
  stored_at: string;
}

// --- Long-form meeting transcription -------------------------------------

export type MeetingStatus =
  | "pending"
  | "processing"
  | "completed"
  | "failed"
  | "canceled";

export type MeetingSegmentStatus = "pending" | "completed" | "failed";

export interface MeetingProgress {
  segments_total: number;
  segments_done: number;
  processed_ms: number;
  elapsed_ms: number;
}

export interface MeetingSegment {
  index: number;
  start_ms: number;
  end_ms: number;
  status: MeetingSegmentStatus;
  text: string;
  no_speech: boolean;
}

export interface MeetingSummary {
  id: string;
  title: string;
  source_filename: string;
  status: MeetingStatus;
  duration_ms: number;
  created_at: string;
  updated_at: string;
  progress: MeetingProgress;
  model_id: string;
  language: TranscriptionLanguage;
  error: string | null;
}

export interface MeetingRecord extends MeetingSummary {
  audio_file: string;
  audio_format: string;
  app_version: string;
  segments: MeetingSegment[];
}

export interface MeetingDetail {
  record: MeetingRecord;
  transcript: string;
  audio_path: string;
}

export interface MeetingProgressPayload {
  id: string;
  status: MeetingStatus;
  segments_total: number;
  segments_done: number;
  processed_ms: number;
  duration_ms: number;
  elapsed_ms: number;
  eta_ms: number;
}

export interface MeetingSegmentPayload {
  id: string;
  index: number;
  start_ms: number;
  end_ms: number;
  text: string;
}

export interface MeetingFinishedPayload {
  id: string;
  status: MeetingStatus;
  error: string | null;
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
  accessibility: PermissionState;
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
  microphone_captures: MicrophoneCaptureStatus[];
  queue: QueueSnapshot;
}

export interface MicrophoneCaptureStatus {
  slot: MicrophoneSlot;
  device_id: string;
  label: string;
  active: boolean;
  reconnecting: boolean;
  warning?: string | null;
  level: LevelSnapshot;
}

export interface AppSnapshot {
  settings: AppSettings;
  status: AppStatus;
  microphones: MicrophoneDevice[];
  models: ModelDescriptor[];
  queue: QueueSnapshot;
  recent_transcripts: RecentTranscript[];
  permissions: AppPermissions;
  openrouter_key_configured: boolean;
  openai_key_configured: boolean;
}
