import { afterEach, describe, expect, it, vi } from "vitest";
import {
  defaultPermissions,
  defaultSettings,
  emptyQueue,
  mockModels,
} from "./app-state";
import type {
  AppStatus,
  LlmReportRunSnapshot,
  QueueSnapshot,
  RecentTranscript,
} from "./types";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.open,
  save: mocks.save,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mocks.listen,
}));

const queue = emptyQueue();
const status: AppStatus = {
  mode: "recording_and_transcription",
  tray_state: "idle",
  live_input_active: false,
  active_model: "whisper-medium",
  active_microphone: "System Default",
  microphone_warning: null,
  runtime_warning: null,
  threshold_dbfs: -40,
  level: {
    current_dbfs: -120,
    peak_dbfs: -120,
    noise_floor_dbfs: -120,
    suggested_threshold_dbfs: -90,
  },
  microphone_captures: [],
  queue,
};

function reportRun(
  overrides: Partial<LlmReportRunSnapshot> = {},
): LlmReportRunSnapshot {
  return {
    run_id: "run-1",
    parent_run_id: null,
    revision: 1,
    status: "queued",
    stage: null,
    kind: "summary",
    created_at: "2026-07-18T00:00:00Z",
    updated_at: "2026-07-18T00:00:00Z",
    started_at: null,
    finished_at: null,
    iteration: 0,
    max_iterations: 3,
    message: "Queued for report generation",
    detail: null,
    error: null,
    progress: [],
    model: "z-ai/glm-5.2",
    selected_count: 1,
    date_range: "2026-07-18",
    report_id: null,
    report_path: null,
    completion_reason: null,
    success_criteria_met: null,
    quality_feedback: null,
    usage: null,
    ...overrides,
  };
}

function mockInvoke(command: string) {
  switch (command) {
    case "get_settings":
      return Promise.resolve(defaultSettings());
    case "app_status":
      return Promise.resolve(status);
    case "list_microphones":
      return Promise.resolve([]);
    case "microphone_input_levels":
      return Promise.resolve([
        {
          device_id: "default",
          label: "System Default",
          volume_percent: 63,
          writable: true,
          available: true,
          error: null,
        },
      ]);
    case "set_microphone_input_volume":
      return Promise.resolve([
        {
          device_id: "default",
          label: "System Default",
          volume_percent: 64,
          writable: true,
          available: true,
          error: null,
        },
      ]);
    case "dictation_state":
      return Promise.resolve({ state: "transcribing", error: null });
    case "cancel_dictation":
      return Promise.resolve({ state: "idle", error: null });
    case "list_models":
      return Promise.resolve(mockModels());
    case "queue_snapshot":
      return Promise.resolve(queue satisfies QueueSnapshot);
    case "openrouter_key_status":
    case "openai_key_status":
    case "soniox_key_status":
      return Promise.resolve({ configured: false });
    case "save_soniox_api_key":
      return Promise.resolve({ configured: true });
    case "delete_soniox_api_key":
      return Promise.resolve({ configured: false });
    case "dictionary_file_status":
    case "reload_dictionary_file":
      return Promise.resolve({
        path: "/tmp/WakeNote/dictionary.txt",
        revision: "abc123",
        error: null,
        error_line: null,
        in_sync: true,
      });
    case "open_dictionary_file":
    case "preview_subtitle":
      return Promise.resolve(null);
    case "regenerate_transcript":
    case "cancel_activity_job":
      return Promise.resolve(queue satisfies QueueSnapshot);
    case "trash_activity_jobs":
      return Promise.resolve({
        queue,
        removed_ids: [7],
        trashed_ids: [7],
        missing_ids: [],
        failures: [],
      });
    case "trash_transcripts":
      return Promise.resolve({
        removed_transcript_paths: ["/tmp/WakeNote/20260611/024304-spotify.txt"],
        trashed_transcript_paths: ["/tmp/WakeNote/20260611/024304-spotify.txt"],
        missing_transcript_paths: [],
        failures: [],
      });
    case "open_transcript_folder":
      return Promise.resolve(null);
    case "manual_meeting_recording_status":
    case "start_manual_meeting_recording":
    case "stop_manual_meeting_recording":
      return Promise.resolve({
        generation: 1,
        state:
          command === "start_manual_meeting_recording" ? "recording" : "off",
        meeting_id: "meeting-manual-1",
        started_at: null,
        elapsed_ms: 0,
        remaining_ms: 18_000_000,
        inputs: ["Microphone", "System Audio"],
        stop_reason: null,
        error: null,
      });
    case "start_meeting_transcription":
    case "import_meeting_recording":
      return Promise.resolve({
        id: "meeting-1",
        title: "Meeting",
        source_filename: "audio.wav",
        status: command === "import_meeting_recording" ? "recorded" : "pending",
        duration_ms: 1_000,
        created_at: "2026-08-03T00:00:00Z",
        updated_at: "2026-08-03T00:00:00Z",
        progress: {
          segments_total: 0,
          segments_done: 0,
          processed_ms: 0,
          elapsed_ms: 0,
        },
        model_id: "openai-gpt-4o-transcribe-diarize",
        language: "ko",
        error: null,
      });
    case "transcription_cost_snapshot":
      return Promise.resolve({
        currency: "USD",
        generated_at: "2026-08-03T00:00:00Z",
        today: {
          estimated_cost_usd: 0.006,
          audio_duration_ms: 60_000,
          request_count: 1,
          unpriced_request_count: 0,
        },
        week: {
          estimated_cost_usd: 0.006,
          audio_duration_ms: 60_000,
          request_count: 1,
          unpriced_request_count: 0,
        },
        month: {
          estimated_cost_usd: 0.006,
          audio_duration_ms: 60_000,
          request_count: 1,
          unpriced_request_count: 0,
        },
        entry_count: 1,
        disclosure: "Local estimate",
      });
    case "permission_snapshot":
      return Promise.resolve(defaultPermissions());
    case "open_permission_drag_shelf":
      return Promise.resolve(defaultPermissions());
    case "recent_transcripts":
      return Promise.resolve([
        {
          transcript_path: "/tmp/WakeNote/20260512/120000.txt",
          audio_path: null,
          recorded_at: "2026-05-12T12:00:00+09:00",
          text: "recent transcript",
        },
      ] satisfies RecentTranscript[]);
    case "rebuild_transcript_day_index":
      return Promise.resolve([
        {
          transcript_path: "/tmp/WakeNote/20260512/120000.txt",
          audio_path: null,
          recorded_at: "2026-05-12T12:00:00+09:00",
          text: "rebuilt transcript",
        },
      ] satisfies RecentTranscript[]);
    case "start_llm_report":
      return Promise.resolve(reportRun());
    case "list_llm_report_runs":
      return Promise.resolve([reportRun()]);
    case "cancel_llm_report":
      return Promise.resolve(
        reportRun({
          revision: 2,
          status: "stopping",
          message: "Stopping report generation",
        }),
      );
    case "retry_llm_report":
      return Promise.resolve(
        reportRun({
          run_id: "run-2",
          parent_run_id: "run-1",
        }),
      );
    default:
      return Promise.reject(new Error(`unexpected invoke command: ${command}`));
  }
}

describe("tauri runtime client snapshots", () => {
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
    vi.resetModules();
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.open.mockReset();
    mocks.save.mockReset();
  });

  it("keeps transcript archive scans out of the fast startup snapshot", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { loadSnapshot } = await import("./tauri-client");

    const snapshot = await loadSnapshot();

    expect(snapshot.recent_transcripts).toEqual([]);
    expect(snapshot.openrouter_key_configured).toBe(false);
    expect(snapshot.openai_key_configured).toBe(false);
    expect(snapshot.soniox_key_configured).toBe(false);
    expect(snapshot.microphone_input_levels[0]?.volume_percent).toBe(63);
    expect(mocks.invoke).toHaveBeenCalledWith("openai_key_status");
    expect(mocks.invoke).toHaveBeenCalledWith("soniox_key_status");
    expect(mocks.invoke).toHaveBeenCalledWith("dictionary_file_status");
    expect(mocks.invoke).toHaveBeenCalledWith("microphone_input_levels");
    expect(mocks.invoke.mock.calls.map(([command]) => command)).not.toContain(
      "recent_transcripts",
    );
  });

  it("sets system input volume through the dedicated command", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { setMicrophoneInputVolume } = await import("./tauri-client");

    const snapshot = await setMicrophoneInputVolume("default", 63.6);

    expect(mocks.invoke).toHaveBeenCalledWith("set_microphone_input_volume", {
      deviceId: "default",
      volumePercent: 64,
    });
    expect(snapshot.microphone_input_levels[0]?.volume_percent).toBe(63);
  });

  it("loads and cancels Dictation through dedicated commands", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { cancelDictation, loadDictationState } = await import(
      "./tauri-client"
    );

    await expect(loadDictationState()).resolves.toEqual({
      state: "transcribing",
      error: null,
    });
    await expect(cancelDictation()).resolves.toEqual({
      state: "idle",
      error: null,
    });
    expect(mocks.invoke).toHaveBeenCalledWith("dictation_state");
    expect(mocks.invoke).toHaveBeenCalledWith("cancel_dictation");
  });

  it("loads recent dictations with a dedicated bounded history command", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockResolvedValue([]);
    const { loadRecentDictations } = await import("./tauri-client");
    await expect(loadRecentDictations()).resolves.toEqual([]);
    expect(mocks.invoke).toHaveBeenCalledWith("recent_dictations", { limit: 10 });
  });

  it("routes supported app-list permissions through the native drag shelf", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const {
      openAccessibilityPermissionAssistant,
      openScreenRecordingPermissionAssistant,
    } = await import("./tauri-client");

    await openAccessibilityPermissionAssistant();
    await openScreenRecordingPermissionAssistant();

    expect(mocks.invoke).toHaveBeenCalledWith("open_permission_drag_shelf", {
      target: "accessibility",
    });
    expect(mocks.invoke).toHaveBeenCalledWith("open_permission_drag_shelf", {
      target: "screen_recording",
    });
  });

  it("loads a fresh permission snapshot without requesting access", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { loadPermissionSnapshot } = await import("./tauri-client");

    await expect(loadPermissionSnapshot()).resolves.toEqual(
      defaultPermissions(),
    );
    expect(mocks.invoke).toHaveBeenCalledWith("permission_snapshot");
    expect(mocks.invoke.mock.calls.map(([command]) => command)).not.toContain(
      "request_microphone_permission",
    );
  });

  it("previews subtitle settings through the native overlay command", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { previewSubtitle } = await import("./tauri-client");
    const patch = {
      floating_overlay_font_size_px: 4,
      floating_overlay_position: "bottom_right" as const,
      subtitle_animation: "dissolve" as const,
      subtitle_padding_horizontal_px: 99,
      subtitle_border_width_px: 12,
      subtitle_border_color: "#12AbEf",
      subtitle_max_width_px: 480,
      subtitle_min_height_px: 900,
      subtitle_max_height_px: 320,
    };

    await previewSubtitle(patch);

    expect(mocks.invoke).toHaveBeenCalledWith("preview_subtitle", {
      patch: {
        ...patch,
        floating_overlay_font_size_px: 10,
        subtitle_padding_horizontal_px: 64,
        subtitle_border_width_px: 8,
        subtitle_border_color: "#12abef",
        subtitle_max_width_px: 480,
        subtitle_min_height_px: 320,
        subtitle_max_height_px: 900,
      },
    });
  });

  it("opens and reloads the stable dictionary file through exact commands", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { openDictionaryFile, reloadDictionaryFile } = await import(
      "./tauri-client"
    );

    await openDictionaryFile();
    await reloadDictionaryFile();

    expect(mocks.invoke).toHaveBeenCalledWith("open_dictionary_file");
    expect(mocks.invoke).toHaveBeenCalledWith("reload_dictionary_file");
  });

  it("saves and deletes the Soniox key through exact commands", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { saveSonioxApiKey, deleteSonioxApiKey } = await import(
      "./tauri-client"
    );

    await saveSonioxApiKey(" soniox-key ");
    await deleteSonioxApiKey();

    expect(mocks.invoke).toHaveBeenCalledWith("save_soniox_api_key", {
      apiKey: " soniox-key ",
    });
    expect(mocks.invoke).toHaveBeenCalledWith("delete_soniox_api_key");
  });

  it("controls manual meeting recording through explicit commands", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const {
      loadManualMeetingRecordingStatus,
      startManualMeetingRecording,
      stopManualMeetingRecording,
    } = await import("./tauri-client");

    expect((await loadManualMeetingRecordingStatus()).state).toBe("off");
    expect((await startManualMeetingRecording()).state).toBe("recording");
    expect((await stopManualMeetingRecording()).state).toBe("off");
    expect(mocks.invoke).toHaveBeenCalledWith(
      "manual_meeting_recording_status",
    );
    expect(mocks.invoke).toHaveBeenCalledWith("start_manual_meeting_recording");
    expect(mocks.invoke).toHaveBeenCalledWith("stop_manual_meeting_recording");
  });

  it("starts recorded meeting transcription with an exact option snapshot", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { startMeetingTranscription } = await import("./tauri-client");
    const request = {
      model_id: "openai-gpt-4o-transcribe-diarize",
      language: "ko" as const,
      streaming_enabled: true,
      speaker_separation_enabled: true,
    };

    const meeting = await startMeetingTranscription("meeting-1", request);

    expect(meeting.status).toBe("pending");
    expect(mocks.invoke).toHaveBeenCalledWith("start_meeting_transcription", {
      id: "meeting-1",
      request,
    });
  });

  it("imports meeting audio without implicitly starting transcription", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    mocks.open.mockResolvedValue("/tmp/meeting.m4a");
    const { importMeetingRecording } = await import("./tauri-client");

    const meeting = await importMeetingRecording();

    expect(meeting?.status).toBe("recorded");
    expect(mocks.invoke).toHaveBeenCalledWith("import_meeting_recording", {
      sourcePath: "/tmp/meeting.m4a",
    });
    expect(mocks.invoke).not.toHaveBeenCalledWith(
      "start_meeting_transcription",
      expect.anything(),
    );
  });

  it("loads the local transcription cost dashboard snapshot", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { loadTranscriptionCostSnapshot } = await import("./tauri-client");

    const snapshot = await loadTranscriptionCostSnapshot();

    expect(snapshot.today.estimated_cost_usd).toBe(0.006);
    expect(mocks.invoke).toHaveBeenCalledWith("transcription_cost_snapshot");
  });

  it("loads recent transcripts through a separate bounded command", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { loadRecentTranscripts } = await import("./tauri-client");

    const transcripts = await loadRecentTranscripts();

    expect(transcripts).toHaveLength(1);
    expect(mocks.invoke).toHaveBeenCalledWith("recent_transcripts", {
      limit: 50,
    });
  });

  it("rebuilds one transcript day index through an explicit Tauri command", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { rebuildTranscriptDayIndex } = await import("./tauri-client");

    const transcripts = await rebuildTranscriptDayIndex("2026-05-12", true);

    expect(transcripts[0].text).toBe("rebuilt transcript");
    expect(mocks.invoke).toHaveBeenCalledWith("rebuild_transcript_day_index", {
      day: "2026-05-12",
      download: true,
    });
  });

  it("chooses a directory and exports every transcript day through Tauri", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.open.mockResolvedValue("/tmp/Exports");
    mocks.invoke.mockResolvedValue({
      destination_path: "/tmp/Exports/WakeNote Transcripts 2026-08-24 153000",
      day_count: 2,
      transcript_count: 3,
    });
    const { exportAllTranscriptDays } = await import("./tauri-client");

    const result = await exportAllTranscriptDays();

    expect(mocks.open).toHaveBeenCalledWith({
      directory: true,
      multiple: false,
      title: "Export all transcript days",
    });
    expect(mocks.invoke).toHaveBeenCalledWith("export_all_transcript_days", {
      destinationParent: "/tmp/Exports",
    });
    expect(result).toEqual({
      destination_path: "/tmp/Exports/WakeNote Transcripts 2026-08-24 153000",
      day_count: 2,
      transcript_count: 3,
    });
  });

  it("chooses a text file and exports the active transcript day through Tauri", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.save.mockResolvedValue("/tmp/WakeNote-2026-08-24.txt");
    mocks.invoke.mockResolvedValue({
      destination_path: "/tmp/WakeNote-2026-08-24.txt",
      day_count: 1,
      transcript_count: 2,
    });
    const { exportTranscriptDay } = await import("./tauri-client");

    const result = await exportTranscriptDay("2026-08-24");

    expect(mocks.save).toHaveBeenCalledWith({
      defaultPath: "WakeNote-2026-08-24.txt",
      filters: [{ name: "Text", extensions: ["txt"] }],
    });
    expect(mocks.invoke).toHaveBeenCalledWith("export_transcript_day", {
      day: "2026-08-24",
      destinationPath: "/tmp/WakeNote-2026-08-24.txt",
    });
    expect(result?.transcript_count).toBe(2);
  });

  it("does not invoke export commands after a cancelled destination dialog", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.open.mockResolvedValue(null);
    mocks.save.mockResolvedValue(null);
    const { exportAllTranscriptDays, exportTranscriptDay } = await import(
      "./tauri-client"
    );

    await expect(exportAllTranscriptDays()).resolves.toBeNull();
    await expect(exportTranscriptDay("2026-08-24")).resolves.toBeNull();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("invokes regenerate_transcript and reloads the Tauri snapshot", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { regenerateTranscript } = await import("./tauri-client");

    await regenerateTranscript(
      "/tmp/WakeNote/20260611/024304-spotify.m4a",
      "whisper-small",
    );

    expect(mocks.invoke).toHaveBeenCalledWith("regenerate_transcript", {
      audioPath: "/tmp/WakeNote/20260611/024304-spotify.m4a",
      modelId: "whisper-small",
    });
    expect(mocks.invoke).toHaveBeenCalledWith("queue_snapshot");
  });

  it("invokes open_transcript_folder for a transcript row path", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { openTranscriptFolder } = await import("./tauri-client");

    await openTranscriptFolder("/tmp/WakeNote/20260611/024304-spotify.m4a");

    expect(mocks.invoke).toHaveBeenCalledWith("open_transcript_folder", {
      path: "/tmp/WakeNote/20260611/024304-spotify.m4a",
    });
  });

  it("moves selected Activity jobs to Trash and reloads the Tauri snapshot", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { trashActivityJobs } = await import("./tauri-client");

    const outcome = await trashActivityJobs([7]);

    expect(outcome.removed_ids).toEqual([7]);
    expect(mocks.invoke).toHaveBeenCalledWith("trash_activity_jobs", {
      ids: [7],
    });
    expect(mocks.invoke).toHaveBeenCalledWith("queue_snapshot");
  });

  it("cancels an exact running Activity job", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { cancelActivityJob } = await import("./tauri-client");

    await cancelActivityJob(17);

    expect(mocks.invoke).toHaveBeenCalledWith("cancel_activity_job", {
      id: 17,
    });
    expect(mocks.invoke).toHaveBeenCalledWith("queue_snapshot");
  });

  it("moves selected Transcript bundles to Trash with exact transcript paths", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { trashTranscripts } = await import("./tauri-client");
    const transcript: RecentTranscript = {
      transcript_path: "/tmp/WakeNote/20260611/024304-spotify.txt",
      audio_path: "/tmp/WakeNote/20260611/024304-spotify.m4a",
      recorded_at: "2026-06-11T02:43:04+09:00",
      text: "Selected transcript",
    };

    const outcome = await trashTranscripts([transcript, transcript]);

    expect(outcome.removed_transcript_paths).toEqual([
      transcript.transcript_path,
    ]);
    expect(mocks.invoke).toHaveBeenCalledWith("trash_transcripts", {
      transcriptPaths: [transcript.transcript_path],
    });
  });

  it("does not start an audio merge when the save dialog is cancelled", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.save.mockResolvedValue(null);
    const { mergeTranscriptAudio } = await import("./tauri-client");
    const entries = [
      {
        transcript_path: "/tmp/one.txt",
        audio_path: "/tmp/one.m4a",
        recorded_at: "2026-08-04T09:00:00+09:00",
        text: "One",
      },
      {
        transcript_path: "/tmp/two.txt",
        audio_path: "/tmp/two.wav",
        recorded_at: "2026-08-04T10:00:00+09:00",
        text: "Two",
      },
    ] satisfies RecentTranscript[];

    await expect(mergeTranscriptAudio(entries, vi.fn())).resolves.toBeNull();

    expect(mocks.save).toHaveBeenCalledOnce();
    expect(mocks.listen).not.toHaveBeenCalled();
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("chooses the destination before invoking merge with timestamps and forwards progress", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    const destinationPath = "/tmp/WakeNote merged.m4a";
    const unlisten = vi.fn();
    let progressHandler:
      | ((event: { payload: Record<string, unknown> }) => void)
      | undefined;
    mocks.save.mockResolvedValue(destinationPath);
    mocks.listen.mockImplementation(async (eventName, handler) => {
      expect(eventName).toBe("audio-merge-progress");
      progressHandler = handler;
      return unlisten;
    });
    mocks.invoke.mockImplementation(async (command, args) => {
      expect(command).toBe("merge_transcript_audio");
      const request = (args as { request: { operation_id: string } }).request;
      progressHandler?.({
        payload: {
          operation_id: request.operation_id,
          stage: "saved",
          completed_inputs: 2,
          total_inputs: 2,
          percent: 100,
          current_input_label: null,
          destination_path: destinationPath,
        },
      });
      return {
        operation_id: request.operation_id,
        destination_path: destinationPath,
        input_count: 2,
      };
    });
    const { mergeTranscriptAudio } = await import("./tauri-client");
    const onProgress = vi.fn();
    const entries = [
      {
        transcript_path: "/tmp/one.txt",
        audio_path: "/tmp/one.m4a",
        recorded_at: "2026-08-04T09:00:00+09:00",
        text: "One",
      },
      {
        transcript_path: "/tmp/two.txt",
        audio_path: "/tmp/two.wav",
        recorded_at: "2026-08-04T10:00:00+09:00",
        text: "Two",
      },
    ] satisfies RecentTranscript[];

    const result = await mergeTranscriptAudio(entries, onProgress);

    expect(mocks.save).toHaveBeenCalledBefore(mocks.invoke);
    expect(mocks.invoke).toHaveBeenCalledWith("merge_transcript_audio", {
      request: expect.objectContaining({
        operation_id: expect.stringMatching(/^merge-[A-Za-z0-9_-]+$/),
        destination_path: destinationPath,
        inputs: [
          {
            audio_path: "/tmp/one.m4a",
            recorded_at: "2026-08-04T09:00:00+09:00",
          },
          {
            audio_path: "/tmp/two.wav",
            recorded_at: "2026-08-04T10:00:00+09:00",
          },
        ],
      }),
    });
    expect(onProgress).toHaveBeenCalledWith(
      expect.objectContaining({ stage: "preparing", percent: 0 }),
    );
    expect(onProgress).toHaveBeenCalledWith(
      expect.objectContaining({ stage: "saved", percent: 100 }),
    );
    expect(result?.destination_path).toBe(destinationPath);
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("uses durable report run commands and forwards revisioned events", async () => {
    (globalThis as { window?: unknown }).window = {
      __TAURI_INTERNALS__: {},
    };
    let runHandler:
      | ((event: { payload: LlmReportRunSnapshot }) => void)
      | undefined;
    const unlisten = vi.fn();
    mocks.invoke.mockImplementation(mockInvoke);
    mocks.listen.mockImplementation(
      async (
        eventName: string,
        handler: (event: { payload: LlmReportRunSnapshot }) => void,
      ) => {
        expect(eventName).toBe("llm-report-run-updated");
        runHandler = handler;
        return unlisten;
      },
    );
    const {
      cancelLlmReport,
      listLlmReportRuns,
      retryLlmReport,
      startLlmReport,
      subscribeLlmReportRuns,
    } = await import("./tauri-client");
    const request = {
      kind: "summary" as const,
      transcripts: [],
    };
    const updates: LlmReportRunSnapshot[] = [];

    const started = await startLlmReport(request);
    const listed = await listLlmReportRuns();
    const stopping = await cancelLlmReport(started.run_id);
    const retry = await retryLlmReport(started.run_id);
    const stopListening = await subscribeLlmReportRuns((run) =>
      updates.push(run),
    );
    runHandler?.({
      payload: reportRun({
        revision: 3,
        status: "cancelled",
        stage: "cancelled",
      }),
    });
    stopListening();

    expect(listed).toHaveLength(1);
    expect(stopping.status).toBe("stopping");
    expect(retry.parent_run_id).toBe(started.run_id);
    expect(updates[0].revision).toBe(3);
    expect(mocks.invoke).toHaveBeenCalledWith("start_llm_report", {
      request,
    });
    expect(mocks.invoke).toHaveBeenCalledWith("cancel_llm_report", {
      runId: "run-1",
    });
    expect(mocks.invoke).toHaveBeenCalledWith("retry_llm_report", {
      runId: "run-1",
    });
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("does not invoke the legacy blocking report command", async () => {
    (globalThis as { window?: unknown }).window = {
      __TAURI_INTERNALS__: {},
    };
    mocks.invoke.mockImplementation(mockInvoke);
    const { startLlmReport } = await import("./tauri-client");

    await startLlmReport({
      kind: "summary",
      transcripts: [],
    });

    expect(mocks.invoke.mock.calls.map(([command]) => command)).not.toContain(
      "generate_transcript_report",
    );
  });

  it("lists and loads persisted LLM report history through Tauri commands", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    const item = {
      report_id: "20260713-100000-summary",
      kind: "summary",
      created_at: "2026-07-13T10:00:00+09:00",
      file_name: "20260713-100000-summary.md",
      report_path: "/tmp/reports/20260713-100000-summary.md",
      model: "z-ai/glm-5.2",
      iterations_used: 1,
      max_iterations: 3,
      success_criteria_met: true,
      completion_reason: "success_criteria_met",
      quality_feedback: "Complete.",
      selected_count: 2,
      date_range: "2026-07-13T09:00:00+09:00 - 2026-07-13T10:00:00+09:00",
      usage: {
        request_count: 2,
        prompt_tokens: 180,
        completion_tokens: 60,
        total_tokens: 240,
        cost: 0.0015,
      },
      legacy: false,
    } as const;
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "list_llm_report_history") return Promise.resolve([item]);
      if (command === "load_llm_report_history_detail") {
        return Promise.resolve({ item, content: "# Saved summary" });
      }
      return mockInvoke(command);
    });
    const { listLlmReportHistory, loadLlmReportHistoryDetail } = await import(
      "./tauri-client"
    );

    const history = await listLlmReportHistory();
    const detail = await loadLlmReportHistoryDetail(item.report_id);

    expect(history).toEqual([item]);
    expect(detail.content).toBe("# Saved summary");
    expect(mocks.invoke).toHaveBeenCalledWith(
      "load_llm_report_history_detail",
      {
        reportId: item.report_id,
      },
    );
  });

  it("downloads an existing report with a native Markdown save dialog", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.save.mockResolvedValue("/tmp/exported-summary.md");
    mocks.invoke.mockResolvedValue("/tmp/exported-summary.md");
    const { downloadLlmReport } = await import("./tauri-client");

    const exported = await downloadLlmReport(
      "20260713-100000-summary",
      "20260713-100000-summary.md",
    );

    expect(mocks.save).toHaveBeenCalledWith({
      defaultPath: "20260713-100000-summary.md",
      filters: [{ name: "Markdown", extensions: ["md"] }],
    });
    expect(mocks.invoke).toHaveBeenCalledWith("export_llm_report", {
      reportId: "20260713-100000-summary",
      destinationPath: "/tmp/exported-summary.md",
    });
    expect(exported).toBe("/tmp/exported-summary.md");
  });
});
