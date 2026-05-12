import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings, emptyQueue, mockModels } from "./app-state";
import type { AppStatus, QueueSnapshot, RecentTranscript } from "./types";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  open: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: mocks.invoke,
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.open,
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
  threshold_dbfs: -60,
  level: {
    current_dbfs: -120,
    peak_dbfs: -120,
    noise_floor_dbfs: -120,
    suggested_threshold_dbfs: -90,
  },
  queue,
};

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
    case "recent_transcripts":
      return Promise.resolve([
        {
          transcript_path: "/tmp/WakeNote/20260512/120000.txt",
          audio_path: null,
          recorded_at: "2026-05-12T12:00:00+09:00",
          text: "recent transcript",
        },
      ] satisfies RecentTranscript[]);
    default:
      return Promise.reject(new Error(`unexpected invoke command: ${command}`));
  }
}

describe("tauri runtime client snapshots", () => {
  afterEach(() => {
    delete (globalThis as { window?: unknown }).window;
    vi.resetModules();
    mocks.invoke.mockReset();
    mocks.open.mockReset();
  });

  it("keeps transcript archive scans out of the fast startup snapshot", async () => {
    (globalThis as { window?: unknown }).window = { __TAURI_INTERNALS__: {} };
    mocks.invoke.mockImplementation(mockInvoke);
    const { loadSnapshot } = await import("./tauri-client");

    const snapshot = await loadSnapshot();

    expect(snapshot.recent_transcripts).toEqual([]);
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
    expect(mocks.invoke).toHaveBeenCalledWith("recent_transcripts", { limit: 50 });
  });
});
