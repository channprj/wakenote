import { describe, expect, it } from "vitest";
import {
  defaultSettings,
  deriveProductMode,
  deriveTrayState,
  mockModels,
  mockSnapshot,
  pollSnapshotDependencyKey,
  shouldRefreshSnapshotForTauriEvent,
  shouldPollSnapshot,
} from "./app-state";

describe("app state derivation", () => {
  it("merges two microphone inputs by default", () => {
    expect(defaultSettings().merge_microphone_inputs).toBe(true);
  });

  it("uses the upstream whisper.cpp repository for mock model downloads", () => {
    const whisperModels = mockModels().filter((model) => model.provider_runtime === "whisper-rs");
    expect(whisperModels.length).toBeGreaterThan(0);
    for (const model of whisperModels) {
      expect(model.download_url).toMatch(
        /^https:\/\/huggingface\.co\/ggerganov\/whisper\.cpp\/resolve\/main\//,
      );
    }
  });

  it("ships dictation off by default with the product shortcut and auto-detect language", () => {
    const settings = defaultSettings();
    expect(settings.dictation_enabled).toBe(false);
    expect(settings.dictation_shortcut).toBe("alt+space");
    expect(settings.dictation_language).toBe("auto");
    expect(settings).toMatchObject({
      dictation_start_sound: "original",
      dictation_stop_sound: "original",
      dictation_end_sound: "original",
      dictation_cue_volume: "medium",
      dictation_bubble_position: "top_center",
      dictation_bubble_background_color: "#000000",
      dictation_bubble_background_opacity: 88,
      dictation_model: "",
      dictation_copy_to_clipboard: true,
      dictation_remove_trailing_space: false,
    });
  });

  it("maps independent recording/transcription toggles to PRD product modes", () => {
    expect(
      deriveProductMode({ recording_enabled: true, transcription_enabled: true, pause_all: false }),
    ).toBe("recording_and_transcription");
    expect(
      deriveProductMode({ recording_enabled: true, transcription_enabled: false, pause_all: false }),
    ).toBe("recording_only");
    expect(
      deriveProductMode({ recording_enabled: false, transcription_enabled: true, pause_all: false }),
    ).toBe("transcription_only");
    expect(
      deriveProductMode({ recording_enabled: false, transcription_enabled: false, pause_all: false }),
    ).toBe("paused");
    expect(
      deriveProductMode({ recording_enabled: true, transcription_enabled: true, pause_all: true }),
    ).toBe("paused");
  });

  it("derives tray states from mode and queue activity", () => {
    expect(deriveTrayState("recording_and_transcription", false)).toBe("idle");
    expect(deriveTrayState("recording_and_transcription", false, false, true)).toBe("listening");
    expect(deriveTrayState("recording_and_transcription", false, false, true, true)).toBe(
      "recording",
    );
    expect(deriveTrayState("recording_and_transcription", true, false, true, true)).toBe(
      "recording",
    );
    expect(deriveTrayState("recording_only", false)).toBe("idle");
    expect(deriveTrayState("recording_only", false, false, true)).toBe("listening");
    expect(deriveTrayState("transcription_only", false)).toBe("idle");
    expect(deriveTrayState("paused", false)).toBe("paused");
    expect(deriveTrayState("recording_only", true)).toBe("transcribing");
    expect(deriveTrayState("recording_only", false, true)).toBe("error");
  });

  it("keeps browser fallback models in sync with built-in model registry options", () => {
    const models = mockModels();
    const ids = models.map((model) => model.id);

    expect(ids).toEqual([
      "whisper-large",
      "whisper-medium",
      "whisper-small",
      "whisper-turbo",
      "parakeet-tdt-0.6b-v3",
      "sensevoice-small",
      "nemotron-3.5-asr-streaming-0.6b",
      "qwen3-asr-0.6b",
      "qwen3-asr-1.7b",
      "openrouter-qwen3-asr-flash",
      "openai-gpt-live-transcribe",
      "openai-gpt-transcribe",
      "openai-gpt-4o-transcribe-diarize",
    ]);
    for (const model of models.filter((model) => model.provider_runtime === "whisper-rs")) {
      expect(model.languages).toContain("ko");
    }
    expect(models.find((model) => model.id === "sensevoice-small")?.languages).toContain("ko");
    expect(
      models.find((model) => model.id === "nemotron-3.5-asr-streaming-0.6b")?.languages,
    ).toContain("ko");
    expect(models.find((model) => model.id === "qwen3-asr-0.6b")?.languages).toContain("ko");
    expect(models.find((model) => model.id === "openrouter-qwen3-asr-flash")).toMatchObject({
      provider_runtime: "openrouter-stt",
      offline: false,
      status: "ready",
    });
    expect(models.find((model) => model.id === "openai-gpt-transcribe")).toMatchObject({
      provider_runtime: "openai-stt",
      offline: false,
      status: "ready",
    });
    expect(models.find((model) => model.id === "openai-gpt-live-transcribe")).toMatchObject({
      provider_runtime: "openai-realtime",
      capabilities: { streaming: "required", realtime: true },
    });
    expect(
      models.find((model) => model.id === "openai-gpt-4o-transcribe-diarize"),
    ).toMatchObject({
      capabilities: { diarization: true, maximum_request_bytes: 25_000_000 },
    });
  });

  it("keeps polling while transcription queue work is pending even if the tray is idle", () => {
    const snapshot = mockSnapshot();
    const idleStatus = { ...snapshot.status, tray_state: "idle" as const };
    const pendingQueue = {
      ...snapshot.queue,
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/imported/pending.wav",
          model_id: "whisper-medium",
          status: "pending" as const,
          error: null,
        },
      ],
      pending_count: 1,
    };
    const readyModels = snapshot.models.map((model) =>
      model.id === "whisper-medium" ? { ...model, status: "ready" as const } : model,
    );

    expect(shouldPollSnapshot(idleStatus, pendingQueue, readyModels)).toBe(true);
    expect(shouldPollSnapshot(idleStatus, { ...snapshot.queue, running_count: 1 })).toBe(true);
    expect(shouldPollSnapshot(idleStatus, snapshot.queue)).toBe(false);
  });

  it("does not poll static pending queue work when every pending job model is unavailable", () => {
    const snapshot = mockSnapshot();
    const idleStatus = { ...snapshot.status, tray_state: "idle" as const };
    const pendingQueue = {
      ...snapshot.queue,
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/imported/missing-model.wav",
          model_id: "whisper-medium",
          status: "pending" as const,
          error: null,
        },
      ],
      pending_count: 1,
    };

    expect(shouldPollSnapshot(idleStatus, pendingQueue, snapshot.models)).toBe(false);
  });

  it("does not keep polling for pending transcription work when transcription is disabled", () => {
    const snapshot = mockSnapshot();
    const pendingQueue = { ...snapshot.queue, pending_count: 1 };
    const recordingOnlyStatus = {
      ...snapshot.status,
      mode: "recording_only" as const,
      tray_state: "idle" as const,
    };
    const pausedStatus = {
      ...snapshot.status,
      mode: "paused" as const,
      tray_state: "paused" as const,
    };

    expect(shouldPollSnapshot(recordingOnlyStatus, pendingQueue, snapshot.models)).toBe(false);
    expect(shouldPollSnapshot(pausedStatus, pendingQueue, snapshot.models)).toBe(false);
    expect(
      shouldPollSnapshot(
        { ...recordingOnlyStatus, tray_state: "transcribing" as const },
        pendingQueue,
        snapshot.models,
      ),
    ).toBe(true);
  });

  it("keeps polling while model download or verification state is active", () => {
    const snapshot = mockSnapshot();
    const idleStatus = { ...snapshot.status, tray_state: "idle" as const };
    const downloadingModels = snapshot.models.map((model) =>
      model.id === "whisper-medium"
        ? { ...model, status: "downloading" as const, download_progress: 24 }
        : model,
    );
    const verifyingModels = snapshot.models.map((model) =>
      model.id === "whisper-medium" ? { ...model, status: "verifying" as const } : model,
    );

    expect(shouldPollSnapshot(idleStatus, snapshot.queue, downloadingModels)).toBe(true);
    expect(shouldPollSnapshot(idleStatus, snapshot.queue, verifyingModels)).toBe(true);
    expect(shouldPollSnapshot(idleStatus, snapshot.queue, snapshot.models)).toBe(false);
  });

  it("keeps polling while a live input stream error is active", () => {
    const snapshot = mockSnapshot();
    const streamErrorStatus = {
      ...snapshot.status,
      tray_state: "error" as const,
      runtime_warning: "Live input stream error: default input stream disconnected",
    };

    expect(shouldPollSnapshot(streamErrorStatus, snapshot.queue, snapshot.models)).toBe(true);
  });

  it("keeps polling while live input remains active under an error state", () => {
    const snapshot = mockSnapshot();
    const activeInputErrorStatus = {
      ...snapshot.status,
      tray_state: "error" as const,
      live_input_active: true,
    };

    expect(shouldPollSnapshot(activeInputErrorStatus, snapshot.queue, snapshot.models)).toBe(true);
  });

  it("keeps polling while launch auto-start is pending from an idle snapshot", () => {
    const snapshot = mockSnapshot();

    expect(
      shouldPollSnapshot(snapshot.status, snapshot.queue, snapshot.models, {
        launchAutoStartPending: true,
      }),
    ).toBe(true);
  });

  it("refreshes snapshots when system-audio capture lifecycle events arrive", () => {
    expect(shouldRefreshSnapshotForTauriEvent("source-capture-started")).toBe(true);
    expect(shouldRefreshSnapshotForTauriEvent("source-capture-stopped")).toBe(true);
    expect(shouldRefreshSnapshotForTauriEvent("source-capture-error")).toBe(true);
    expect(shouldRefreshSnapshotForTauriEvent("source-detected")).toBe(false);
    expect(shouldRefreshSnapshotForTauriEvent("live-transcript-partial")).toBe(false);
  });

  it("changes the polling dependency key when polling inputs change", () => {
    const snapshot = mockSnapshot();
    const baseKey = pollSnapshotDependencyKey(snapshot.status, snapshot.queue, snapshot.models);

    expect(
      pollSnapshotDependencyKey(
        { ...snapshot.status, live_input_active: true },
        snapshot.queue,
        snapshot.models,
      ),
    ).not.toBe(baseKey);
    expect(
      pollSnapshotDependencyKey(
        { ...snapshot.status, runtime_warning: "Live input stream error: default input stream disconnected" },
        snapshot.queue,
        snapshot.models,
      ),
    ).not.toBe(baseKey);
    expect(
      pollSnapshotDependencyKey(
        { ...snapshot.status, mode: "recording_only" },
        snapshot.queue,
        snapshot.models,
      ),
    ).not.toBe(baseKey);
  });

  it("changes the polling dependency key when pending job model targets change", () => {
    const snapshot = mockSnapshot();
    const mediumPendingQueue = {
      ...snapshot.queue,
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/imported/medium.wav",
          model_id: "whisper-medium",
          status: "pending" as const,
          error: null,
        },
      ],
      pending_count: 1,
    };
    const tinyPendingQueue = {
      ...mediumPendingQueue,
      jobs: [
        {
          id: 2,
          audio_path: "/tmp/imported/tiny.wav",
          model_id: "whisper-small",
          status: "pending" as const,
          error: null,
        },
      ],
    };

    expect(
      pollSnapshotDependencyKey(snapshot.status, tinyPendingQueue, snapshot.models),
    ).not.toBe(pollSnapshotDependencyKey(snapshot.status, mediumPendingQueue, snapshot.models));
  });

  it("provides browser-safe mock data for Vite development outside Tauri", () => {
    const snapshot = mockSnapshot();

    expect(snapshot.settings.save_root).toBe("~/Documents/WakeNote");
    expect(snapshot.settings.model_directory).toBe("~/Library/Application Support/WakeNote/models");
    expect(snapshot.settings.threshold_dbfs).toBe(-40);
    expect(snapshot.settings.mic_input_volume_percent).toBe(100);
    expect(snapshot.settings.attack_ms).toBe(200);
    expect(snapshot.settings.release_ms).toBe(1_000);
    expect(snapshot.settings.pre_roll_ms).toBe(400);
    expect(snapshot.settings.lead_in_padding_ms).toBe(200);
    expect(snapshot.settings.post_roll_ms).toBe(400);
    expect(snapshot.settings.min_chunk_ms).toBe(800);
    expect(snapshot.settings.max_chunk_ms).toBe(180_000);
    expect(snapshot.settings.transcription_language).toBe("ko");
    expect(snapshot.settings.suppress_low_confidence_transcripts).toBe(true);
    expect(snapshot.settings.show_dock_icon).toBe(true);
    expect(snapshot.settings.show_tray_icon).toBe(true);
    expect(snapshot.settings.start_live_input_on_launch).toBe(true);
    expect(snapshot.settings.theme_mode).toBe("dark");
    expect(snapshot.settings.theme_primary_color).toBe("#000");
    expect(snapshot.microphones[0].id).toBe("default");
    expect(snapshot.models.map((model) => model.id)).toContain("whisper-medium");
    expect(snapshot.status.threshold_dbfs).toBe(-40);
    expect(snapshot.status.microphone_warning).toBeNull();
    expect(snapshot.status.level).toEqual({
      current_dbfs: -120,
      peak_dbfs: -120,
      noise_floor_dbfs: -120,
      suggested_threshold_dbfs: -90,
    });
    expect(snapshot.recent_transcripts).toEqual([]);
  });
});
