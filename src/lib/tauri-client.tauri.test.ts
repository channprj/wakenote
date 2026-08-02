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
    case "list_models":
      return Promise.resolve(mockModels());
    case "queue_snapshot":
      return Promise.resolve(queue satisfies QueueSnapshot);
    case "openrouter_key_status":
    case "openai_key_status":
      return Promise.resolve({ configured: false });
    case "regenerate_transcript":
      return Promise.resolve(queue satisfies QueueSnapshot);
    case "open_transcript_folder":
      return Promise.resolve(null);
    case "permission_snapshot":
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
    expect(mocks.invoke).toHaveBeenCalledWith("openai_key_status");
    expect(mocks.invoke.mock.calls.map(([command]) => command)).not.toContain(
      "recent_transcripts",
    );
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
