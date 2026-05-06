import type {
  AppMode,
  AppSettings,
  AppSnapshot,
  AppStatus,
  LevelSnapshot,
  ModelDescriptor,
  QueueSnapshot,
  TrayState,
} from "./types";

type ModeInput = Pick<
  AppSettings,
  "recording_enabled" | "transcription_enabled" | "pause_all"
>;

export function deriveProductMode(settings: ModeInput): AppMode {
  if (
    settings.pause_all ||
    (!settings.recording_enabled && !settings.transcription_enabled)
  ) {
    return "paused";
  }

  if (settings.recording_enabled && settings.transcription_enabled) {
    return "recording_and_transcription";
  }

  if (settings.recording_enabled) {
    return "recording_only";
  }

  return "transcription_only";
}

export function deriveTrayState(
  mode: AppMode,
  hasRunningTranscription: boolean,
  hasError = false,
): TrayState {
  if (hasRunningTranscription) {
    return "transcribing";
  }

  if (hasError) {
    return "error";
  }

  switch (mode) {
    case "recording_and_transcription":
    case "recording_only":
      return "listening";
    case "transcription_only":
      return "idle";
    case "paused":
      return "paused";
  }
}

export function shouldShowFloatingOverlay(
  settings: Pick<AppSettings, "show_floating_overlay">,
  trayState: TrayState,
) {
  return (
    settings.show_floating_overlay &&
    ["listening", "recording", "transcribing"].includes(trayState)
  );
}

export function shouldPollSnapshot(status: AppStatus, queue: QueueSnapshot) {
  return (
    ["listening", "recording", "transcribing"].includes(status.tray_state) ||
    queue.pending_count > 0 ||
    queue.running_count > 0
  );
}

export function defaultSettings(): AppSettings {
  return {
    recording_enabled: true,
    transcription_enabled: true,
    pause_all: false,
    selected_microphone: "default",
    selected_microphone_label: "System Default",
    save_root: "~/Documents/Sagwan",
    audio_format: "m4a",
    threshold_dbfs: -45,
    attack_ms: 300,
    release_ms: 1500,
    pre_roll_ms: 300,
    post_roll_ms: 300,
    min_chunk_ms: 500,
    max_chunk_ms: 300000,
    selected_model: "whisper-medium",
    model_directory: "~/Library/Application Support/Sagwan/models",
    vad_enabled: false,
    launch_at_login: false,
    show_tray_icon: true,
    show_floating_overlay: true,
    theme_primary_color: "#0047AB",
  };
}

export function mockModels(): ModelDescriptor[] {
  return [
    {
      id: "whisper-medium",
      display_name: "Whisper Medium",
      engine: "whisper.cpp",
      provider_runtime: "whisper-rs",
      download_url:
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
      checksum_sha256:
        "6c14d5adee4f86394037d23e1625d96385c22f032d72d6fdf045dc1741ca091e",
      size_mb: 1465,
      languages: ["ko", "en", "multi"],
      speed_score: 5,
      accuracy_score: 8,
      offline: true,
      status: "missing",
      download_progress: null,
      download_error: null,
    },
    {
      id: "whisper-tiny",
      display_name: "Whisper Tiny",
      engine: "whisper.cpp",
      provider_runtime: "whisper-rs",
      download_url:
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin",
      checksum_sha256:
        "bd577a113a864445d4c299885e0cb97d4ba92b5fca5b2bce5b656d95d0f941a2",
      size_mb: 75,
      languages: ["ko", "en", "multi"],
      speed_score: 9,
      accuracy_score: 4,
      offline: true,
      status: "missing",
      download_progress: null,
      download_error: null,
    },
  ];
}

export function emptyQueue(): QueueSnapshot {
  return {
    jobs: [],
    pending_count: 0,
    running_count: 0,
    failed_count: 0,
  };
}

export function defaultLevelSnapshot(): LevelSnapshot {
  return {
    current_dbfs: -120,
    peak_dbfs: -120,
    noise_floor_dbfs: -120,
    suggested_threshold_dbfs: -90,
  };
}

export function mockSnapshot(): AppSnapshot {
  const settings = defaultSettings();
  const queue = emptyQueue();
  const mode = deriveProductMode(settings);
  const trayState = deriveTrayState(mode, queue.running_count > 0);

  return {
    settings,
    status: {
      mode,
      tray_state: trayState,
      active_model: settings.selected_model,
      active_microphone: settings.selected_microphone_label,
      microphone_warning: null,
      threshold_dbfs: settings.threshold_dbfs,
      level: defaultLevelSnapshot(),
      queue,
    },
    microphones: [
      {
        id: "default",
        label: "System Default",
        available: true,
        fallback: false,
      },
      {
        id: "builtin-input",
        label: "Built-in Microphone",
        available: true,
        fallback: false,
      },
    ],
    models: mockModels(),
    queue,
  };
}
