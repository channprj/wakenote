import { describe, expect, it } from "vitest";
import {
  captureStatusPresentation,
  nextDismissedWarningKey,
  queueCardTone,
  visibleWarningForDismissedKey,
} from "./status-summary";
import { mockSnapshot } from "./app-state";

describe("capture status presentation", () => {
  it("describes stopped input with microphone and threshold context", () => {
    const snapshot = mockSnapshot();

    expect(captureStatusPresentation(snapshot)).toMatchObject({
      headline: "Input stopped",
      tone: "neutral",
      modeLabel: "Recording + transcription",
      microphone: "System Default",
      threshold: "-60 dBFS",
      queueSummary: "0 pending · 0 running · 0 failed",
    });
  });

  it("uses silence levels for stopped input summaries", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        status: {
          ...snapshot.status,
          live_input_active: false,
          level: {
            current_dbfs: -22,
            peak_dbfs: -18,
            noise_floor_dbfs: -55,
            suggested_threshold_dbfs: -43,
          },
        },
      }).levelSummary,
    ).toBe("-120 dBFS current · -120 dBFS peak");
  });

  it("describes continuous listening only when capture is active", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        status: { ...snapshot.status, tray_state: "listening" },
      }),
    ).toMatchObject({
      headline: "Listening for voice",
      tone: "primary",
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

  it("labels idle recording modes as stopped input", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        status: {
          ...snapshot.status,
          mode: "recording_and_transcription",
          tray_state: "idle",
        },
      }),
    ).toMatchObject({ headline: "Input stopped", tone: "neutral" });
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

  it("hides only the currently dismissed warning key", () => {
    const warning = {
      key: "runtime:Live input stream error: default input stream disconnected",
      message: "Live input stream error: default input stream disconnected",
      tone: "danger" as const,
    };

    expect(visibleWarningForDismissedKey(warning, warning.key)).toBeNull();
    expect(visibleWarningForDismissedKey(warning, "runtime:other")).toEqual(warning);
  });

  it("clears dismissed warning state after warnings recover", () => {
    expect(nextDismissedWarningKey(null, "runtime:Live input stream error")).toBeNull();
    expect(nextDismissedWarningKey({ key: "runtime:a", message: "a", tone: "warning" }, "runtime:a")).toBe(
      "runtime:a",
    );
  });
});

describe("queueCardTone", () => {
  it("returns undefined when no failed jobs are present", () => {
    expect(queueCardTone({ failed_count: 0 })).toBeUndefined();
  });

  it("returns 'danger' when at least one failed job is present", () => {
    expect(queueCardTone({ failed_count: 1 })).toBe("danger");
    expect(queueCardTone({ failed_count: 42 })).toBe("danger");
  });
});
