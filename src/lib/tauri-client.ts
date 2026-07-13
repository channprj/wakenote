import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { formatLocalTimestamp } from "./transcript-history";
import {
  defaultSettings,
  defaultLevelSnapshot,
  deriveProductMode,
  deriveTrayState,
  emptyQueue,
  mockModels,
  mockSnapshot,
  defaultPermissions,
} from "./app-state";
import type {
  AppSettings,
  AppSnapshot,
  AppStatus,
  MicrophoneDevice,
  ModelDescriptor,
  QueueJob,
  QueueSnapshot,
  RecentTranscript,
  SettingsPatch,
  TranscriptDay,
  UploadedAudio,
  AudioWaveform,
  AppPermissions,
  RecognizedSourceInfo,
  SourceCaptureStatus,
  SourcePayload,
  MeetingSummary,
  MeetingDetail,
  LlmGenerateRequest,
  LlmGenerateResponse,
  OpenRouterKeyStatus,
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

let browserSnapshot = mockSnapshot();
let browserCaptureSessionId = 0;
let browserQueuedCaptureSessionId: number | null = null;
let browserCaptureSessionTranscriptionRequested = false;
// Simulated system-audio capture state for non-Tauri dev (locked-behaviors §10).
// `startSourceCapture` flips it on (and simulates detecting that source);
// `stopSourceCapture` flips it off so dev reflects session active/inactive.
let browserSourceCapturing = false;
let browserDetectedSource: SourcePayload | null = null;
let browserOpenRouterApiKey: string | null = null;
const browserVerificationPreviousStatuses = new Map<string, ModelDescriptor["status"]>();
const defaultRecentTranscriptLimit = 50;

function permissionSnapshotFromBrowser(): AppPermissions {
  return browserSnapshot.permissions ?? defaultPermissions();
}

export function isTauriRuntime() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

function queueFromJobs(jobs: QueueJob[]): QueueSnapshot {
  return {
    jobs,
    pending_count: jobs.filter((job) => job.status === "pending").length,
    running_count: jobs.filter((job) => job.status === "running").length,
    failed_count: jobs.filter((job) => job.status === "failed").length,
  };
}

function isBrowserImportableAudioPath(audioPath: string) {
  return /\.(m4a|wav)$/i.test(audioPath);
}

function isBrowserUploadableAudioPath(audioPath: string) {
  return /\.(mp3|m4a|wav)$/i.test(audioPath);
}

function isUsableBrowserModel(modelId: string, models: ModelDescriptor[]) {
  const model = models.find((candidate) => candidate.id === modelId);
  return Boolean(model && ["ready", "installed", "unloaded"].includes(model.status));
}

function settledBrowserModels(models: ModelDescriptor[]): ModelDescriptor[] {
  return models.map((model) => {
    if (model.status === "downloading") {
      return {
        ...model,
        status: "ready",
        download_progress: 100,
        download_error: null,
      };
    }

    if (model.status !== "verifying") {
      return model;
    }

    const previousStatus = browserVerificationPreviousStatuses.get(model.id) ?? "missing";
    browserVerificationPreviousStatuses.delete(model.id);
    if (["ready", "installed", "unloaded"].includes(previousStatus)) {
      return {
        ...model,
        status: "ready",
        download_progress: 100,
        download_error: null,
      };
    }

    return {
      ...model,
      status: previousStatus === "error" ? "error" : "missing",
      download_progress: null,
      download_error:
        previousStatus === "error" ? (model.download_error ?? "model verification failed") : null,
    };
  });
}

function browserLevelSnapshot(activeCapture: boolean): AppStatus["level"] {
  if (!activeCapture) {
    return defaultLevelSnapshot();
  }

  const now = Date.now();
  const currentDbfs = Math.max(
    -78,
    Math.min(-62, -70 + Math.sin(now / 320) * 7 + Math.sin(now / 95) * 2),
  );

  return {
    current_dbfs: currentDbfs,
    peak_dbfs: Math.max(currentDbfs, browserSnapshot.status?.level.peak_dbfs ?? currentDbfs),
    noise_floor_dbfs: -73,
    suggested_threshold_dbfs: -61,
  };
}

function maybeQueueBrowserCapture(settings: AppSettings, queue: QueueSnapshot, status: AppStatus) {
  if (
    browserCaptureSessionId === 0 ||
    browserQueuedCaptureSessionId === browserCaptureSessionId ||
    !browserCaptureSessionTranscriptionRequested ||
    status.tray_state !== "recording"
  ) {
    return queue;
  }

  const audioPath = `${settings.save_root}/browser-capture-${String(browserCaptureSessionId).padStart(
    3,
    "0",
  )}.${settings.audio_format}`;
  browserQueuedCaptureSessionId = browserCaptureSessionId;
  if (queue.jobs.some((job) => job.audio_path === audioPath)) {
    return queue;
  }

  return queueFromJobs([
    ...queue.jobs,
    {
      id: queue.jobs.length + 1,
      audio_path: audioPath,
      model_id: settings.selected_model,
      status: "pending",
      error: null,
    },
  ]);
}

function statusFrom(
  settings: AppSettings,
  queue: QueueSnapshot,
  liveInputActive = browserSnapshot.status?.live_input_active ?? false,
): AppStatus {
  const mode = deriveProductMode(settings);
  const canCapture =
    mode === "recording_and_transcription" || mode === "recording_only";
  const activeCapture = canCapture && liveInputActive;
  const level = browserLevelSnapshot(activeCapture);
  const isRecording = activeCapture && level.current_dbfs >= settings.threshold_dbfs;

  return {
    mode,
    tray_state: deriveTrayState(
      mode,
      queue.running_count > 0,
      queue.failed_count > 0,
      activeCapture,
      isRecording,
    ),
    live_input_active: activeCapture,
    active_model: settings.selected_model,
    active_microphone: settings.selected_microphone_label,
    microphone_warning: browserSnapshot.status?.microphone_warning ?? null,
    runtime_warning: browserSnapshot.status?.runtime_warning ?? null,
    threshold_dbfs: settings.threshold_dbfs,
    level,
    queue,
  };
}

export async function loadSnapshot(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const currentQueue = browserSnapshot.queue ?? emptyQueue();
    const models = settledBrowserModels(browserSnapshot.models ?? mockModels());
    const currentStatus = statusFrom(settings, currentQueue);
    const queue = maybeQueueBrowserCapture(settings, currentQueue, currentStatus);
    browserSnapshot = {
      ...browserSnapshot,
      settings,
      models,
      queue,
      status: statusFrom(settings, queue),
      openrouter_key_configured: Boolean(browserOpenRouterApiKey),
    };
    return browserSnapshot;
  }

  const [settings, status, microphones, models, queue, permissions, openRouterKeyStatus] = await Promise.all([
    invoke<AppSettings>("get_settings"),
    invoke<AppStatus>("app_status"),
    invoke<MicrophoneDevice[]>("list_microphones"),
    invoke<ModelDescriptor[]>("list_models"),
    invoke<QueueSnapshot>("queue_snapshot"),
    invoke<AppPermissions>("permission_snapshot"),
    invoke<OpenRouterKeyStatus>("openrouter_key_status"),
  ]);

  return {
    settings,
    status,
    microphones,
    models,
    queue,
    permissions,
    recent_transcripts: [],
    openrouter_key_configured: openRouterKeyStatus.configured,
  };
}

export async function loadPermissions(): Promise<AppPermissions> {
  if (!isTauriRuntime()) {
    return permissionSnapshotFromBrowser();
  }

  return invoke<AppPermissions>("permission_snapshot");
}

export async function loadRecentTranscripts(
  limit = defaultRecentTranscriptLimit,
): Promise<RecentTranscript[]> {
  if (!isTauriRuntime()) {
    return browserSnapshot.recent_transcripts ?? [];
  }

  return invoke<RecentTranscript[]>("recent_transcripts", { limit });
}

function transcriptDayFromBrowser(transcript: RecentTranscript): string {
  return formatLocalTimestamp(transcript.recorded_at).slice(0, 10);
}

// Returns days in ascending order; the backend (BTreeMap) and this browser
// fallback agree. Consumers that need newest-first should sort themselves
// (TranscriptsView derives its own ordering).
export async function loadTranscriptDays(): Promise<TranscriptDay[]> {
  if (!isTauriRuntime()) {
    const counts = new Map<string, number>();
    for (const transcript of browserSnapshot.recent_transcripts ?? []) {
      const day = transcriptDayFromBrowser(transcript);
      if (day) {
        counts.set(day, (counts.get(day) ?? 0) + 1);
      }
    }
    return [...counts]
      .map(([day, count]) => ({ day, count }))
      .sort((left, right) => left.day.localeCompare(right.day));
  }

  return invoke<TranscriptDay[]>("transcript_days");
}

// `download` lets an explicit reload fetch iCloud-evicted sidecars; the default
// (on-navigation load) leaves them on the server and shows only local files.
export async function loadTranscriptsForDay(
  day: string,
  download = false,
): Promise<RecentTranscript[]> {
  if (!isTauriRuntime()) {
    return (browserSnapshot.recent_transcripts ?? []).filter(
      (transcript) => transcriptDayFromBrowser(transcript) === day,
    );
  }

  return invoke<RecentTranscript[]>("transcripts_for_day", { day, download });
}

export async function rebuildTranscriptDayIndex(
  day: string,
  download = true,
): Promise<RecentTranscript[]> {
  if (!isTauriRuntime()) {
    return loadTranscriptsForDay(day, download);
  }

  return invoke<RecentTranscript[]>("rebuild_transcript_day_index", { day, download });
}

export async function saveSettingsPatch(patch: SettingsPatch): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const previousSettings = browserSnapshot.settings ?? defaultSettings();
    const models = browserSnapshot.models ?? mockModels();
    const safePatch = { ...patch };
    if (
      typeof safePatch.selected_model === "string" &&
      !isUsableBrowserModel(safePatch.selected_model, models)
    ) {
      delete safePatch.selected_model;
    }
    if (typeof safePatch.save_root === "string") {
      safePatch.save_root_confirmed = safePatch.save_root.trim().length > 0;
    }
    if (typeof safePatch.llm_max_iterations === "number") {
      safePatch.llm_max_iterations = Math.max(
        1,
        Math.min(30, Math.round(safePatch.llm_max_iterations)),
      );
    }
    let queue = browserSnapshot.queue ?? emptyQueue();
    if (
      safePatch.recording_enabled === false ||
      safePatch.pause_all === true ||
      safePatch.transcription_enabled === false
    ) {
      queue = maybeQueueBrowserCapture(
        previousSettings,
        queue,
        statusFrom(previousSettings, queue),
      );
    }

    const settings = { ...previousSettings, ...safePatch };
    browserSnapshot = {
      ...browserSnapshot,
      settings,
      models,
      status: statusFrom(settings, queue),
      queue,
      openrouter_key_configured: Boolean(browserOpenRouterApiKey),
    };
    return browserSnapshot;
  }

  await invoke<AppSettings>("update_settings", { patch });
  return loadSnapshot();
}

export async function saveOpenRouterApiKey(apiKey: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const trimmed = apiKey.trim();
    if (!trimmed) {
      throw new Error("OpenRouter API key cannot be blank");
    }
    browserOpenRouterApiKey = trimmed;
    browserSnapshot = {
      ...browserSnapshot,
      openrouter_key_configured: true,
    };
    return loadSnapshot();
  }

  await invoke<OpenRouterKeyStatus>("save_openrouter_api_key", { apiKey });
  return loadSnapshot();
}

export async function deleteOpenRouterApiKey(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    browserOpenRouterApiKey = null;
    browserSnapshot = {
      ...browserSnapshot,
      openrouter_key_configured: false,
    };
    return loadSnapshot();
  }

  await invoke<OpenRouterKeyStatus>("delete_openrouter_api_key");
  return loadSnapshot();
}

export async function generateTranscriptReport(
  request: LlmGenerateRequest,
): Promise<LlmGenerateResponse> {
  const settings = browserSnapshot.settings ?? defaultSettings();
  if (!isTauriRuntime()) {
    if (!browserOpenRouterApiKey) {
      throw new Error("OpenRouter API key is not configured");
    }
    const label = request.kind === "summary" ? "Summary" : "Detailed report";
    return {
      content: `# ${label}\n\nGenerated from ${request.transcripts.length} transcript${request.transcripts.length === 1 ? "" : "s"}.`,
      iterations_used: settings.llm_max_iterations,
      model: settings.openrouter_model,
      report_path: `${settings.save_root}/reports/browser-${request.kind}.md`,
    };
  }

  return invoke<LlmGenerateResponse>("generate_transcript_report", { request });
}

export async function enqueueBacklog(saveRoot: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const audioPath = `${saveRoot}/20260506/230709.m4a`;
    if (browserSnapshot.queue.jobs.some((job) => job.audio_path === audioPath)) {
      return browserSnapshot;
    }

    const id = browserSnapshot.queue.jobs.length + 1;
    const queue: QueueSnapshot = {
      jobs: [
        ...browserSnapshot.queue.jobs,
        {
          id,
          audio_path: audioPath,
          model_id: settings.selected_model,
          status: "pending",
          error: null,
        },
      ],
      pending_count: browserSnapshot.queue.pending_count + 1,
      running_count: browserSnapshot.queue.running_count,
      failed_count: browserSnapshot.queue.failed_count,
    };
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
    };
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("enqueue_backlog", { saveRoot });
  return loadSnapshot();
}

export async function enqueueAudioFiles(audioPaths: string[]): Promise<AppSnapshot> {
  if (audioPaths.length === 0) {
    return loadSnapshot();
  }

  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const firstId = browserSnapshot.queue.jobs.length + 1;
    const queuedPaths = new Set(browserSnapshot.queue.jobs.map((job) => job.audio_path));
    const importedJobs = audioPaths
      .filter((audioPath) => {
        if (!isBrowserImportableAudioPath(audioPath)) {
          return false;
        }
        if (queuedPaths.has(audioPath)) {
          return false;
        }
        queuedPaths.add(audioPath);
        return true;
      })
      .map((audioPath, index) => ({
        id: firstId + index,
        audio_path: audioPath,
        model_id: settings.selected_model,
        status: "pending" as const,
        error: null,
      }));
    if (importedJobs.length === 0) {
      return browserSnapshot;
    }

    const queue: QueueSnapshot = {
      jobs: [...browserSnapshot.queue.jobs, ...importedJobs],
      pending_count: browserSnapshot.queue.pending_count + importedJobs.length,
      running_count: browserSnapshot.queue.running_count,
      failed_count: browserSnapshot.queue.failed_count,
    };
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
    };
    return browserSnapshot;
  }

  for (const audioPath of audioPaths) {
    await invoke<QueueSnapshot>("enqueue_audio_file", { audioPath });
  }
  return loadSnapshot();
}

export async function chooseAudioFiles(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  const selected = await open({
    multiple: true,
    title: "Choose Audio Files",
    filters: [
      {
        name: "Audio",
        extensions: ["m4a", "wav"],
      },
    ],
  });
  if (!Array.isArray(selected)) {
    return loadSnapshot();
  }

  return enqueueAudioFiles(selected);
}

export async function requestMicrophonePermission(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    browserSnapshot = {
      ...browserSnapshot,
      permissions: {
        ...(browserSnapshot.permissions ?? defaultPermissions()),
        microphone: {
          status: "granted",
          label: "Allowed",
          detail: "WakeNote can use the microphone.",
          can_request: false,
          can_open_settings: true,
        },
      },
    };
    return loadSnapshot();
  }

  await invoke<AppPermissions>("request_microphone_permission");
  return loadSnapshot();
}

export async function openMicrophonePermissionSettings(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return loadSnapshot();
  }

  await invoke("open_microphone_permission_settings");
  return loadSnapshot();
}

export async function requestScreenRecordingPermission(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    browserSnapshot = {
      ...browserSnapshot,
      permissions: {
        ...(browserSnapshot.permissions ?? defaultPermissions()),
        screen_recording: {
          status: "granted",
          label: "Allowed",
          detail: "WakeNote can capture system audio (Google Meet, Zoom, YouTube).",
          can_request: false,
          can_open_settings: true,
        },
      },
    };
    return loadSnapshot();
  }

  await invoke<AppPermissions>("request_screen_recording_permission");
  return loadSnapshot();
}

export async function openScreenRecordingSettings(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return loadSnapshot();
  }

  await invoke("open_screen_recording_settings");
  return loadSnapshot();
}

// Mirrors the Rust built-in recognized-source list so browser/mock dev keeps
// parity with `list_recognized_sources` (locked-behaviors §10).
const BROWSER_RECOGNIZED_SOURCES: ReadonlyArray<{
  id: string;
  label: string;
  description: string;
  titlePatterns: string[];
  defaultAutoPrompt: boolean;
  custom: boolean;
}> = [
  {
    id: "meet",
    label: "Google Meet",
    description: "Google Meet or browser tabs with Meet in the title",
    titlePatterns: ["google meet", "meet - "],
    defaultAutoPrompt: true,
    custom: false,
  },
  {
    id: "zoom",
    label: "Zoom",
    description: "Zoom desktop app or windows titled Zoom Meeting",
    titlePatterns: ["zoom", "zoom meeting"],
    defaultAutoPrompt: true,
    custom: false,
  },
  {
    id: "youtube",
    label: "YouTube",
    description: "YouTube tabs, videos, and YouTube Music windows",
    titlePatterns: ["- youtube", "youtube"],
    defaultAutoPrompt: true,
    custom: false,
  },
];

function recognizedSourcesFromBrowser(settings: AppSettings): RecognizedSourceInfo[] {
  const builtIns = BROWSER_RECOGNIZED_SOURCES.map((source) => {
    const override = settings.source_auto_prompt.find((entry) => entry.source_id === source.id);
    return {
      id: source.id,
      label: source.label,
      description: source.description,
      auto_prompt: override ? override.auto_prompt : source.defaultAutoPrompt,
      title_patterns: source.titlePatterns,
      custom: source.custom,
    };
  });
  return [
    ...builtIns,
    ...settings.custom_sources.map((source) => {
      const override = settings.source_auto_prompt.find((entry) => entry.source_id === source.id);
      return {
        id: source.id,
        label: source.label,
        description: source.title_patterns.join(", "),
        auto_prompt: override ? override.auto_prompt : source.auto_prompt,
        title_patterns: source.title_patterns,
        custom: true,
      };
    }),
  ];
}

export async function loadRecognizedSources(): Promise<RecognizedSourceInfo[]> {
  if (!isTauriRuntime()) {
    return recognizedSourcesFromBrowser(browserSnapshot.settings ?? defaultSettings());
  }
  return invoke<RecognizedSourceInfo[]>("list_recognized_sources");
}

export async function startSourceCapture(sourceId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const source = recognizedSourcesFromBrowser(settings).find((entry) => entry.id === sourceId);
    if (source) {
      browserDetectedSource = {
        source_id: source.id,
        label: source.label,
        app_name: source.label,
      };
      browserSourceCapturing = true;
    }
    return loadSnapshot();
  }
  await invoke("start_source_capture", { sourceId });
  return loadSnapshot();
}

export async function stopSourceCapture(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    browserSourceCapturing = false;
    return loadSnapshot();
  }
  await invoke("stop_source_capture");
  return loadSnapshot();
}

export async function loadSourceCaptureStatus(): Promise<SourceCaptureStatus> {
  if (!isTauriRuntime()) {
    return {
      detected: browserDetectedSource,
      capturing: browserSourceCapturing,
    };
  }
  return invoke<SourceCaptureStatus>("source_capture_status");
}

export function browserUploadedPathFromSource(
  sourcePath: string,
  saveRoot: string,
  today = new Date(),
): string {
  const fileName = sourcePath.split("/").filter(Boolean).pop() ?? "audio.mp3";
  const day = [
    today.getFullYear(),
    String(today.getMonth() + 1).padStart(2, "0"),
    String(today.getDate()).padStart(2, "0"),
  ].join("");
  return `${saveRoot}/uploaded/${day}/${fileName}`;
}

export async function uploadAudioFile(sourcePath: string): Promise<UploadedAudio> {
  if (!isTauriRuntime()) {
    if (!isBrowserUploadableAudioPath(sourcePath)) {
      throw new Error("only mp3, m4a, and wav audio files can be uploaded");
    }
    const settings = browserSnapshot.settings ?? defaultSettings();
    return {
      audio_path: browserUploadedPathFromSource(sourcePath, settings.save_root),
      original_filename: sourcePath.split("/").filter(Boolean).pop() ?? "audio.mp3",
      stored_at: new Date().toISOString(),
    };
  }

  return invoke<UploadedAudio>("upload_audio_file", { sourcePath });
}

export async function chooseUploadedAudioFile(): Promise<UploadedAudio | null> {
  if (!isTauriRuntime()) {
    return uploadAudioFile("/tmp/browser-upload/meeting.mp3");
  }

  const selected = await open({
    multiple: false,
    title: "Upload Audio",
    filters: [
      {
        name: "Audio",
        extensions: ["mp3", "m4a", "wav"],
      },
    ],
  });
  if (typeof selected !== "string") {
    return null;
  }

  return uploadAudioFile(selected);
}

export async function loadAudioWaveform(
  audioPath: string,
  bucketCount = 4096,
): Promise<AudioWaveform> {
  if (!isTauriRuntime()) {
    const count = Math.min(bucketCount, 512);
    const peaks: number[] = [];
    const peaksMax: number[] = [];
    const peaksMin: number[] = [];
    for (let index = 0; index < count; index += 1) {
      const envelope = Math.abs(Math.sin(index / 7) * Math.cos(index / 19));
      const wobble = Math.sin(index / 3.1) * 0.18;
      const positive = Number((0.06 + envelope * 0.78 + Math.max(0, wobble)).toFixed(3));
      const negative = Number((-0.05 - envelope * 0.66 + Math.min(0, wobble)).toFixed(3));
      peaks.push(Math.min(1, Math.max(positive, Math.abs(negative))));
      peaksMax.push(Math.min(1, positive));
      peaksMin.push(Math.max(-1, negative));
    }
    return {
      duration_seconds: 30 * 60,
      sample_rate: 8000,
      peaks,
      peaks_max: peaksMax,
      peaks_min: peaksMin,
      audible_ranges: [
        { start: 2, end: 320 },
        { start: 370, end: 880 },
        { start: 930, end: 1780 },
      ],
    };
  }

  return invoke<AudioWaveform>("analyze_audio_waveform", { audioPath, bucketCount });
}

export async function cancelCurrentTranscription(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    let cancelled = false;
    const jobs = browserSnapshot.queue.jobs.map((job) => {
      if (cancelled || job.status !== "running") {
        return job;
      }

      cancelled = true;
      return { ...job, status: "cancelled" as const, error: "cancelled by user" };
    });
    if (!cancelled) {
      return browserSnapshot;
    }

    const settings = browserSnapshot.settings ?? defaultSettings();
    const queue = queueFromJobs(jobs);
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
    };
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("cancel_current_transcription");
  return loadSnapshot();
}

export async function cancelCurrentOperation(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const activeModel = browserSnapshot.models.find((model) =>
      ["downloading", "verifying", "extracting"].includes(model.status),
    );
    if (activeModel) {
      browserVerificationPreviousStatuses.delete(activeModel.id);
      browserSnapshot = {
        ...browserSnapshot,
        models: browserSnapshot.models.map((model) =>
          model.id === activeModel.id
            ? {
                ...model,
                status: "error",
                download_progress: model.download_progress ?? 0,
                download_error: "cancelled by user",
              }
            : model,
        ),
      };
      return browserSnapshot;
    }

    return cancelCurrentTranscription();
  }

  await invoke("cancel_current_operation");
  return loadSnapshot();
}

export async function processNextTranscription(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const models = browserSnapshot.models ?? mockModels();
    if (settings.pause_all || !settings.transcription_enabled) {
      return browserSnapshot;
    }
    if (browserSnapshot.queue.jobs.some((job) => job.status === "running")) {
      return browserSnapshot;
    }

    let processedAudioPath = "";
    const jobs = browserSnapshot.queue.jobs.map((job) => {
      if (processedAudioPath || job.status !== "pending" || !isUsableBrowserModel(job.model_id, models)) {
        return job;
      }

      processedAudioPath = job.audio_path;
      return { ...job, status: "completed" as const, error: null };
    });
    if (processedAudioPath.length === 0) {
      return browserSnapshot;
    }

    const queue = queueFromJobs(jobs);
    const transcriptPath = processedAudioPath.replace(/\.(m4a|wav)$/i, ".txt");
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
      recent_transcripts: [
        {
          transcript_path: transcriptPath,
          audio_path: processedAudioPath,
          recorded_at: new Date().toISOString(),
          text: `Browser fallback transcript for ${processedAudioPath.split("/").pop()}`,
        },
        ...(browserSnapshot.recent_transcripts ?? []),
      ],
    };
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("process_next_transcription");
  return loadSnapshot();
}

export async function regenerateTranscript(
  audioPath: string,
  modelId?: string,
): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    if (!isBrowserImportableAudioPath(audioPath)) {
      return browserSnapshot;
    }
    const settings = browserSnapshot.settings ?? defaultSettings();
    const models = browserSnapshot.models ?? mockModels();
    const regenerationModelId =
      modelId && isUsableBrowserModel(modelId, models)
        ? modelId
        : settings.selected_model;
    let matched = false;
    let blocked = false;
    let changed = false;
    const jobs = browserSnapshot.queue.jobs.map((job) => {
      if (job.audio_path !== audioPath) {
        return job;
      }
      matched = true;
      if (job.status === "running") {
        blocked = true;
        return job;
      }
      changed = true;
      return {
        ...job,
        model_id: regenerationModelId,
        status: "pending" as const,
        error: null,
      };
    });
    if (blocked) {
      return browserSnapshot;
    }
    if (!matched) {
      const nextId = Math.max(0, ...jobs.map((job) => job.id)) + 1;
      jobs.push({
        id: nextId,
        audio_path: audioPath,
        model_id: regenerationModelId,
        status: "pending",
        error: null,
      });
      changed = true;
    }
    if (!changed) {
      return browserSnapshot;
    }

    const queue = queueFromJobs(jobs);
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
      recent_transcripts: (browserSnapshot.recent_transcripts ?? []).filter(
        (entry) => entry.audio_path !== audioPath,
      ),
    };
    return browserSnapshot;
  }

  const payload: { audioPath: string; modelId?: string } = { audioPath };
  if (modelId) {
    payload.modelId = modelId;
  }
  await invoke<QueueSnapshot>("regenerate_transcript", payload);
  return loadSnapshot();
}

export async function openTranscriptFolder(path: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return loadSnapshot();
  }

  await invoke("open_transcript_folder", { path });
  return loadSnapshot();
}

export async function startLiveCapture(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const queue = browserSnapshot.queue ?? emptyQueue();
    if (browserSnapshot.status?.live_input_active) {
      return loadSnapshot();
    }
    const mode = deriveProductMode(settings);
    const canCapture = mode === "recording_and_transcription" || mode === "recording_only";
    if (!canCapture) {
      browserSnapshot = {
        ...browserSnapshot,
        settings,
        queue,
        status: statusFrom(settings, queue, false),
      };
      return browserSnapshot;
    }

    browserCaptureSessionId += 1;
    browserCaptureSessionTranscriptionRequested = settings.transcription_enabled;
    browserSnapshot = {
      ...browserSnapshot,
      settings,
      queue,
      status: statusFrom(settings, queue, true),
    };
    return browserSnapshot;
  }

  await invoke<AppStatus>("start_live_capture");
  return loadSnapshot();
}

export async function stopLiveCapture(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const currentQueue = browserSnapshot.queue ?? emptyQueue();
    const queue = maybeQueueBrowserCapture(
      settings,
      currentQueue,
      statusFrom(settings, currentQueue),
    );
    browserSnapshot = {
      ...browserSnapshot,
      settings,
      queue,
      status: statusFrom(settings, queue, false),
    };
    return browserSnapshot;
  }

  await invoke<AppStatus>("stop_live_capture");
  return loadSnapshot();
}

export async function chooseSaveRoot(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  const selected = await open({
    directory: true,
    multiple: false,
    title: "Choose WakeNote Save Folder",
  });
  if (typeof selected !== "string") {
    return loadSnapshot();
  }

  await invoke<AppSettings>("update_settings", {
    patch: { save_root: selected },
  });
  return loadSnapshot();
}

export async function chooseModelDirectory(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  const selected = await open({
    directory: true,
    multiple: false,
    title: "Choose Model Directory",
  });
  if (typeof selected !== "string") {
    return loadSnapshot();
  }

  await invoke<AppSettings>("update_settings", {
    patch: { model_directory: selected },
  });
  return loadSnapshot();
}

export async function revealSaveFolder(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  await invoke("reveal_save_folder");
  return loadSnapshot();
}

export async function verifyModel(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const target = browserSnapshot.models.find((model) => model.id === modelId);
    if (
      !target ||
      ["downloading", "verifying", "extracting"].includes(target.status)
    ) {
      return browserSnapshot;
    }

    browserSnapshot = {
      ...browserSnapshot,
      models: browserSnapshot.models.map((model) =>
        model.id === modelId
          ? (() => {
              browserVerificationPreviousStatuses.set(modelId, model.status);
              return {
                ...model,
                status: "verifying",
                download_progress: null,
                download_error: null,
              };
            })()
          : model,
      ),
    };
    return browserSnapshot;
  }

  await invoke("verify_model", { modelId });
  return loadSnapshot();
}

export async function downloadModel(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const target = browserSnapshot.models.find((model) => model.id === modelId);
    if (
      !target?.download_url ||
      !["missing", "error"].includes(target.status)
    ) {
      return browserSnapshot;
    }

    browserVerificationPreviousStatuses.delete(modelId);
    browserSnapshot = {
      ...browserSnapshot,
      models: browserSnapshot.models.map((model) =>
        model.id === modelId
          ? {
              ...model,
              status: "downloading",
              download_progress: 0,
              download_error: null,
            }
          : model,
      ),
    };
    return browserSnapshot;
  }

  await invoke("download_model", { modelId });
  return loadSnapshot();
}

export async function cancelModelDownload(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const target = browserSnapshot.models.find((model) => model.id === modelId);
    if (
      !target ||
      !["downloading", "verifying", "extracting"].includes(target.status)
    ) {
      return browserSnapshot;
    }

    browserVerificationPreviousStatuses.delete(modelId);
    browserSnapshot = {
      ...browserSnapshot,
      models: browserSnapshot.models.map((model) =>
        model.id === modelId
          ? {
              ...model,
              status: "error",
              download_progress: model.download_progress ?? 0,
              download_error: "cancelled by user",
            }
          : model,
      ),
    };
    return browserSnapshot;
  }

  await invoke("cancel_model_download", { modelId });
  return loadSnapshot();
}

export async function deleteModel(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    const settings = browserSnapshot.settings ?? defaultSettings();
    const target = browserSnapshot.models.find((model) => model.id === modelId);
    if (
      !target ||
      settings.selected_model === modelId ||
      target.status === "missing" ||
      ["downloading", "verifying", "extracting"].includes(target.status)
    ) {
      return browserSnapshot;
    }

    browserVerificationPreviousStatuses.delete(modelId);
    browserSnapshot = {
      ...browserSnapshot,
      models: browserSnapshot.models.map((model) =>
        model.id === modelId
          ? { ...model, status: "missing", download_progress: null, download_error: null }
          : model,
      ),
    };
    return browserSnapshot;
  }

  await invoke("delete_model", { modelId });
  return loadSnapshot();
}

export async function retryJob(id: number): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    let retried = false;
    const jobs = browserSnapshot.queue.jobs.map((job) => {
      if (job.id !== id || !["failed", "cancelled"].includes(job.status)) {
        return job;
      }

      retried = true;
      return { ...job, status: "pending" as const, error: null };
    });
    if (!retried) {
      return browserSnapshot;
    }

    const settings = browserSnapshot.settings ?? defaultSettings();
    const queue = queueFromJobs(jobs);
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
    };
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("retry_job", { id });
  return loadSnapshot();
}

export async function skipJob(id: number): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    let skipped = false;
    const jobs = browserSnapshot.queue.jobs.map((job) => {
      if (job.id !== id || !["pending", "failed", "cancelled"].includes(job.status)) {
        return job;
      }

      skipped = true;
      return { ...job, status: "skipped" as const, error: null };
    });
    if (!skipped) {
      return browserSnapshot;
    }

    const settings = browserSnapshot.settings ?? defaultSettings();
    const queue = queueFromJobs(jobs);
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
    };
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("skip_job", { id });
  return loadSnapshot();
}

// --- Long-form meeting transcription -------------------------------------

export async function listMeetings(): Promise<MeetingSummary[]> {
  if (!isTauriRuntime()) {
    return [];
  }
  return invoke<MeetingSummary[]>("list_meetings");
}

/** Open a file picker for a long recording and start batch transcription. */
export async function importAndStartMeeting(): Promise<MeetingSummary | null> {
  if (!isTauriRuntime()) {
    return null;
  }
  const selected = await open({
    multiple: false,
    title: "Choose meeting recording",
    filters: [{ name: "Audio", extensions: ["mp3", "m4a", "wav"] }],
  });
  if (typeof selected !== "string") {
    return null;
  }
  return invoke<MeetingSummary>("import_and_start_meeting", { sourcePath: selected });
}

export async function meetingDetail(id: string): Promise<MeetingDetail> {
  return invoke<MeetingDetail>("meeting_detail", { id });
}

export async function cancelMeeting(id: string): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  await invoke("cancel_meeting", { id });
}

export async function resumeMeeting(id: string): Promise<MeetingSummary> {
  return invoke<MeetingSummary>("resume_meeting", { id });
}

export async function deleteMeeting(id: string): Promise<void> {
  if (!isTauriRuntime()) {
    return;
  }
  await invoke("delete_meeting", { id });
}
