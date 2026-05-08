import { describe, expect, it } from "vitest";
import {
  deriveProductMode,
  deriveTrayState,
  mockSnapshot,
  shouldPollSnapshot,
  shouldShowFloatingOverlay,
} from "./app-state";

describe("app state derivation", () => {
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
    expect(deriveTrayState("recording_only", false)).toBe("idle");
    expect(deriveTrayState("recording_only", false, false, true)).toBe("listening");
    expect(deriveTrayState("transcription_only", false)).toBe("idle");
    expect(deriveTrayState("paused", false)).toBe("paused");
    expect(deriveTrayState("recording_only", true)).toBe("transcribing");
    expect(deriveTrayState("recording_only", false, true)).toBe("error");
  });

  it("shows the floating overlay only for active capture or transcription states", () => {
    expect(shouldShowFloatingOverlay({ show_floating_overlay: true }, "listening")).toBe(false);
    expect(shouldShowFloatingOverlay({ show_floating_overlay: true }, "recording")).toBe(true);
    expect(shouldShowFloatingOverlay({ show_floating_overlay: true }, "transcribing")).toBe(true);
    expect(shouldShowFloatingOverlay({ show_floating_overlay: true }, "idle")).toBe(false);
    expect(shouldShowFloatingOverlay({ show_floating_overlay: false }, "recording")).toBe(false);
  });

  it("keeps polling while transcription queue work is pending even if the tray is idle", () => {
    const snapshot = mockSnapshot();
    const idleStatus = { ...snapshot.status, tray_state: "idle" as const };

    expect(shouldPollSnapshot(idleStatus, { ...snapshot.queue, pending_count: 1 })).toBe(true);
    expect(shouldPollSnapshot(idleStatus, { ...snapshot.queue, running_count: 1 })).toBe(true);
    expect(shouldPollSnapshot(idleStatus, snapshot.queue)).toBe(false);
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

  it("provides browser-safe mock data for Vite development outside Tauri", () => {
    const snapshot = mockSnapshot();

    expect(snapshot.settings.theme_primary_color).toBe("#0047AB");
    expect(snapshot.microphones[0].id).toBe("default");
    expect(snapshot.models.map((model) => model.id)).toContain("whisper-medium");
    expect(snapshot.status.threshold_dbfs).toBe(-45);
    expect(snapshot.status.microphone_warning).toBeNull();
    expect(snapshot.status.level).toEqual({
      current_dbfs: -120,
      peak_dbfs: -120,
      noise_floor_dbfs: -120,
      suggested_threshold_dbfs: -90,
    });
  });
});
