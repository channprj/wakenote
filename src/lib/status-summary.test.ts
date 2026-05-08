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
      warning: {
        key: "runtime:Live input dropped 42 stale audio frames while processing was busy",
        message: "Live input dropped 42 stale audio frames while processing was busy",
        tone: "warning",
      },
    });
  });

  it("surfaces live input stream errors as danger warnings", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        status: {
          ...snapshot.status,
          runtime_warning: "Live input stream error: default input stream disconnected",
        },
      }).warning,
    ).toEqual({
      key: "runtime:Live input stream error: default input stream disconnected",
      message: "Live input stream error: default input stream disconnected",
      tone: "danger",
    });
  });

  it("prioritizes microphone warnings over runtime and queue warnings", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        queue: { ...snapshot.queue, failed_count: 2 },
        status: {
          ...snapshot.status,
          microphone_warning: "Pinned microphone is unavailable",
          runtime_warning: "Live input dropped 42 stale audio frames while processing was busy",
        },
      }).warning,
    ).toEqual({
      key: "microphone:Pinned microphone is unavailable",
      message: "Pinned microphone is unavailable",
      tone: "danger",
    });
  });

  it("falls back to failed queue warnings when device and runtime are healthy", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        queue: { ...snapshot.queue, failed_count: 3 },
        status: {
          ...snapshot.status,
          queue: { ...snapshot.queue, failed_count: 3 },
        },
      }).warning,
    ).toEqual({
      key: "queue-failed:3",
      message: "3 transcription jobs need attention",
      tone: "danger",
    });
  });
});
