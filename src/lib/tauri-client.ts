import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import {
  defaultSettings,
  defaultLevelSnapshot,
  deriveProductMode,
  deriveTrayState,
  emptyQueue,
  mockModels,
  mockSnapshot,
} from "./app-state";
import type {
  AppSettings,
  AppSnapshot,
  AppStatus,
  MicrophoneDevice,
  ModelDescriptor,
  QueueJob,
  QueueSnapshot,
  SettingsPatch,
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
const browserVerificationPreviousStatuses = new Map<string, ModelDescriptor["status"]>();

function isTauriRuntime() {
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
    -64,
    Math.min(-48, -56 + Math.sin(now / 320) * 7 + Math.sin(now / 95) * 2),
  );

  return {
    current_dbfs: currentDbfs,
    peak_dbfs: Math.max(currentDbfs, browserSnapshot.status?.level.peak_dbfs ?? currentDbfs),
    noise_floor_dbfs: -58,
    suggested_threshold_dbfs: -46,
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
    };
    return browserSnapshot;
  }

  const [settings, status, microphones, models, queue] = await Promise.all([
    invoke<AppSettings>("get_settings"),
    invoke<AppStatus>("app_status"),
    invoke<MicrophoneDevice[]>("list_microphones"),
    invoke<ModelDescriptor[]>("list_models"),
    invoke<QueueSnapshot>("queue_snapshot"),
  ]);

  return { settings, status, microphones, models, queue };
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
      status: statusFrom(settings, queue),
      queue,
    };
    return browserSnapshot;
  }

  await invoke<AppSettings>("update_settings", { patch });
  return loadSnapshot();
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

    let processed = false;
    const jobs = browserSnapshot.queue.jobs.map((job) => {
      if (processed || job.status !== "pending" || !isUsableBrowserModel(job.model_id, models)) {
        return job;
      }

      processed = true;
      return { ...job, status: "completed" as const, error: null };
    });
    if (!processed) {
      return browserSnapshot;
    }

    const queue = queueFromJobs(jobs);
    browserSnapshot = {
      ...browserSnapshot,
      queue,
      status: statusFrom(settings, queue),
    };
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("process_next_transcription");
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
    title: "Choose Sagwan Save Folder",
  });
  if (typeof selected !== "string") {
    return loadSnapshot();
  }

  await invoke<AppSettings>("update_settings", {
    patch: { save_root: selected },
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
