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
  QueueSnapshot,
  SettingsPatch,
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

let browserSnapshot = mockSnapshot();

function isTauriRuntime() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

function statusFrom(settings: AppSettings, queue: QueueSnapshot): AppStatus {
  const mode = deriveProductMode(settings);
  return {
    mode,
    tray_state: deriveTrayState(mode, queue.running_count > 0, queue.failed_count > 0),
    live_input_active: browserSnapshot.status?.live_input_active ?? false,
    active_model: settings.selected_model,
    active_microphone: settings.selected_microphone_label,
    microphone_warning: browserSnapshot.status?.microphone_warning ?? null,
    runtime_warning: browserSnapshot.status?.runtime_warning ?? null,
    threshold_dbfs: settings.threshold_dbfs,
    level: browserSnapshot.status?.level ?? defaultLevelSnapshot(),
    queue,
  };
}

export async function loadSnapshot(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
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
    const settings = { ...browserSnapshot.settings, ...patch };
    const queue = browserSnapshot.queue ?? emptyQueue();
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
    const id = browserSnapshot.queue.jobs.length + 1;
    const queue: QueueSnapshot = {
      jobs: [
        ...browserSnapshot.queue.jobs,
        {
          id,
          audio_path: `${saveRoot}/20260506/230709.m4a`,
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
    const importedJobs = audioPaths.map((audioPath, index) => ({
      id: firstId + index,
      audio_path: audioPath,
      model_id: settings.selected_model,
      status: "pending" as const,
      error: null,
    }));
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
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("cancel_current_transcription");
  return loadSnapshot();
}

export async function processNextTranscription(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("process_next_transcription");
  return loadSnapshot();
}

export async function startLiveCapture(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  await invoke<AppStatus>("start_live_capture");
  return loadSnapshot();
}

export async function stopLiveCapture(): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
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

export async function verifyModel(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    browserSnapshot = {
      ...browserSnapshot,
      models: browserSnapshot.models.map((model) =>
        model.id === modelId ? { ...model, status: "verifying" } : model,
      ),
    };
    return browserSnapshot;
  }

  await invoke("verify_model", { modelId });
  return loadSnapshot();
}

export async function downloadModel(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  await invoke("download_model", { modelId });
  return loadSnapshot();
}

export async function cancelModelDownload(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  await invoke("cancel_model_download", { modelId });
  return loadSnapshot();
}

export async function deleteModel(modelId: string): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    browserSnapshot = {
      ...browserSnapshot,
      models: browserSnapshot.models.map((model) =>
        model.id === modelId ? { ...model, status: "missing" } : model,
      ),
    };
    return browserSnapshot;
  }

  await invoke("delete_model", { modelId });
  return loadSnapshot();
}

export async function retryJob(id: number): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("retry_job", { id });
  return loadSnapshot();
}

export async function skipJob(id: number): Promise<AppSnapshot> {
  if (!isTauriRuntime()) {
    return browserSnapshot;
  }

  await invoke<QueueSnapshot>("skip_job", { id });
  return loadSnapshot();
}
