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
  hasActiveCapture = false,
  isRecording = false,
): TrayState {
  if (isRecording) {
    return "recording";
  }

  if (hasRunningTranscription) {
    return "transcribing";
  }

  if (hasError) {
    return "error";
  }

  switch (mode) {
    case "recording_and_transcription":
    case "recording_only":
      return hasActiveCapture ? "listening" : "idle";
    case "transcription_only":
      return "idle";
    case "paused":
      return "paused";
  }
}

function hasProcessablePendingTranscription(queue: QueueSnapshot, models: ModelDescriptor[]) {
  const usableModelIds = new Set(
    models
      .filter((model) => ["ready", "installed", "unloaded"].includes(model.status))
      .map((model) => model.id),
  );

  return queue.jobs.some(
    (job) => job.status === "pending" && usableModelIds.has(job.model_id),
  );
}

export function shouldPollSnapshot(
  status: AppStatus,
  queue: QueueSnapshot,
  models: ModelDescriptor[] = [],
  options: { launchAutoStartPending?: boolean } = {},
) {
  const canProcessPendingTranscription =
    status.mode === "recording_and_transcription" || status.mode === "transcription_only";

  return (
    options.launchAutoStartPending ||
    ["listening", "recording", "transcribing"].includes(status.tray_state) ||
    status.live_input_active ||
    status.runtime_warning?.startsWith("Live input stream error:") ||
    (canProcessPendingTranscription && hasProcessablePendingTranscription(queue, models)) ||
    queue.running_count > 0 ||
    models.some((model) => ["downloading", "verifying", "extracting"].includes(model.status))
  );
}

export function pollSnapshotDependencyKey(
  status: AppStatus,
  queue: QueueSnapshot,
  models: ModelDescriptor[] = [],
  options: { launchAutoStartPending?: boolean } = {},
) {
  return JSON.stringify({
    mode: status.mode,
    trayState: status.tray_state,
    liveInputActive: status.live_input_active,
    runtimeWarning: status.runtime_warning ?? null,
    launchAutoStartPending: Boolean(options.launchAutoStartPending),
    pendingCount: queue.pending_count,
    runningCount: queue.running_count,
    pendingJobs: queue.jobs
      .filter((job) => job.status === "pending")
      .map((job) => ({ id: job.id, modelId: job.model_id })),
    models: models.map((model) => ({
      id: model.id,
      status: model.status,
      downloadProgress: model.download_progress ?? null,
    })),
  });
}

export function defaultSettings(): AppSettings {
  return {
    recording_enabled: true,
    transcription_enabled: true,
    transcription_language: "ko",
    suppress_low_confidence_transcripts: true,
    pause_all: false,
    selected_microphone: "default",
    selected_microphone_label: "System Default",
    system_mic_volume_enabled: true,
    system_mic_volume: 100,
    save_root: "~/Documents/WakeNote",
    save_root_confirmed: false,
    audio_format: "m4a",
    threshold_dbfs: -60,
    calibration_completed: false,
    attack_ms: 200,
    release_ms: 1500,
    pre_roll_ms: 400,
    post_roll_ms: 400,
    min_chunk_ms: 500,
    max_chunk_ms: 120000,
    selected_model: "whisper-medium",
    model_directory: "~/Library/Application Support/WakeNote/models",
    vad_enabled: false,
    launch_at_login: false,
    start_live_input_on_launch: true,
    show_dock_icon: true,
    show_tray_icon: true,
    show_floating_overlay: true,
    floating_overlay_position: "top",
    theme_primary_color: "#000",
  };
}

export function mockModels(): ModelDescriptor[] {
  return [
    {
      id: "whisper-large",
      display_name: "Whisper Large",
      engine: "whisper.cpp",
      provider_runtime: "whisper-rs",
      download_url: "https://blob.handy.computer/ggml-large-v3-q5_0.bin",
      checksum_sha256:
        "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1",
      size_mb: 1031,
      languages: ["ko", "en", "multi"],
      speed_score: 3,
      accuracy_score: 9,
      offline: true,
      status: "missing",
      download_progress: null,
      download_error: null,
    },
    {
      id: "whisper-medium",
      display_name: "Whisper Medium",
      engine: "whisper.cpp",
      provider_runtime: "whisper-rs",
      download_url:
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
      checksum_sha256:
        "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
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
      id: "whisper-small",
      display_name: "Whisper Small",
      engine: "whisper.cpp",
      provider_runtime: "whisper-rs",
      download_url: "https://blob.handy.computer/ggml-small.bin",
      checksum_sha256:
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
      size_mb: 465,
      languages: ["ko", "en", "multi"],
      speed_score: 8,
      accuracy_score: 6,
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
        "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
      size_mb: 75,
      languages: ["ko", "en", "multi"],
      speed_score: 9,
      accuracy_score: 4,
      offline: true,
      status: "missing",
      download_progress: null,
      download_error: null,
    },
    {
      id: "whisper-turbo",
      display_name: "Whisper Turbo",
      engine: "whisper.cpp",
      provider_runtime: "whisper-rs",
      download_url:
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
      checksum_sha256:
        "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
      size_mb: 1550,
      languages: ["ko", "en", "multi"],
      speed_score: 8,
      accuracy_score: 7,
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
      live_input_active: false,
      active_model: settings.selected_model,
      active_microphone: settings.selected_microphone_label,
      microphone_warning: null,
      runtime_warning: null,
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
    recent_transcripts: [],
  };
}
