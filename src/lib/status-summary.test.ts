import { describe, expect, it } from "vitest";
import { captureStatusPresentation } from "./status-summary";
import { mockSnapshot } from "./app-state";

describe("capture status presentation", () => {
  it("describes continuous listening with microphone and threshold context", () => {
    const snapshot = mockSnapshot();

    expect(captureStatusPresentation(snapshot)).toMatchObject({
      headline: "Listening for voice",
      tone: "primary",
      modeLabel: "Recording + transcription",
      microphone: "System Default",
      threshold: "-45 dBFS",
      queueSummary: "0 pending · 0 running · 0 failed",
    });
  });

  it("prioritizes active recording and transcription states", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        status: { ...snapshot.status, tray_state: "recording" },
      }).headline,
    ).toBe("Recording speech");
    expect(
      captureStatusPresentation({
        ...snapshot,
        status: { ...snapshot.status, tray_state: "transcribing" },
      }).headline,
    ).toBe("Transcribing queue");
  });

  it("surfaces paused and error states as operator-visible warnings", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        status: { ...snapshot.status, tray_state: "paused" },
      }),
    ).toMatchObject({ headline: "Paused", tone: "warning" });
    expect(
      captureStatusPresentation({
        ...snapshot,
        status: { ...snapshot.status, tray_state: "error" },
      }),
    ).toMatchObject({ headline: "Needs attention", tone: "danger" });
  });

  it("includes runtime warnings in the status detail", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        status: {
          ...snapshot.status,
          runtime_warning: "Live input dropped 42 stale audio frames while processing was busy",
        },
      }),
    ).toMatchObject({
      runtimeWarning: "Live input dropped 42 stale audio frames while processing was busy",
    });
  });
});
