import { describe, expect, it } from "vitest";
import { deriveProductMode, deriveTrayState, mockSnapshot } from "./app-state";

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
    expect(deriveTrayState("recording_and_transcription", false)).toBe("listening");
    expect(deriveTrayState("recording_only", false)).toBe("listening");
    expect(deriveTrayState("transcription_only", false)).toBe("idle");
    expect(deriveTrayState("paused", false)).toBe("paused");
    expect(deriveTrayState("recording_only", true)).toBe("transcribing");
  });

  it("provides browser-safe mock data for Vite development outside Tauri", () => {
    const snapshot = mockSnapshot();

    expect(snapshot.settings.theme_primary_color).toBe("#0047AB");
    expect(snapshot.microphones[0].id).toBe("default");
    expect(snapshot.models.map((model) => model.id)).toContain("whisper-medium");
    expect(snapshot.status.threshold_dbfs).toBe(-45);
    expect(snapshot.status.level).toEqual({
      current_dbfs: -120,
      peak_dbfs: -120,
      noise_floor_dbfs: -120,
      suggested_threshold_dbfs: -90,
    });
  });
});
