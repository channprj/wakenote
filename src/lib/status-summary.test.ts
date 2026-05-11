import { describe, expect, it } from "vitest";
import {
  captureStatusPresentation,
  levelCardTone,
  modelStatusBadgeTone,
  nextDismissedWarningKey,
  queueCardTone,
  queueJobStatusBadgeTone,
  queueStatsCellTone,
  runtimeCardTone,
  trayStateBadgeTone,
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
      queueSummary: "0 pending · 0 running · 0 failed · 0 completed",
      queueCompletedCount: 0,
    });
  });

  it("counts completed jobs from queue.jobs and surfaces them in the queue summary", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation({
        ...snapshot,
        queue: {
          ...snapshot.queue,
          jobs: [
            { id: 1, audio_path: "/tmp/a.m4a", model_id: "m", status: "completed" },
            { id: 2, audio_path: "/tmp/b.m4a", model_id: "m", status: "completed" },
            { id: 3, audio_path: "/tmp/c.m4a", model_id: "m", status: "pending" },
          ],
        },
      }),
    ).toMatchObject({
      queueSummary: "0 pending · 0 running · 0 failed · 2 completed",
      queueCompletedCount: 2,
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
  it("returns undefined when queue is fully clean (no failed, running, pending, or completed)", () => {
    expect(
      queueCardTone({ failed_count: 0, running_count: 0, pending_count: 0 }, 0),
    ).toBeUndefined();
  });

  it("returns 'danger' when at least one failed job is present", () => {
    expect(
      queueCardTone({ failed_count: 1, running_count: 0, pending_count: 0 }, 0),
    ).toBe("danger");
    expect(
      queueCardTone({ failed_count: 42, running_count: 0, pending_count: 0 }, 0),
    ).toBe("danger");
  });

  it("returns 'primary' when running jobs exist and no failed jobs", () => {
    expect(
      queueCardTone({ failed_count: 0, running_count: 1, pending_count: 0 }, 0),
    ).toBe("primary");
    expect(
      queueCardTone({ failed_count: 0, running_count: 3, pending_count: 5 }, 0),
    ).toBe("primary");
  });

  it("returns 'warning' when only pending jobs exist", () => {
    expect(
      queueCardTone({ failed_count: 0, running_count: 0, pending_count: 1 }, 0),
    ).toBe("warning");
    expect(
      queueCardTone({ failed_count: 0, running_count: 0, pending_count: 8 }, 0),
    ).toBe("warning");
  });

  it("returns 'success' when only completed jobs exist (happy-path throughput signal)", () => {
    expect(
      queueCardTone({ failed_count: 0, running_count: 0, pending_count: 0 }, 1),
    ).toBe("success");
    expect(
      queueCardTone({ failed_count: 0, running_count: 0, pending_count: 0 }, 17),
    ).toBe("success");
  });

  it("prioritizes danger over running, pending, and completed", () => {
    expect(
      queueCardTone({ failed_count: 1, running_count: 1, pending_count: 1 }, 1),
    ).toBe("danger");
  });

  it("prioritizes running over pending and completed when no failed jobs", () => {
    expect(
      queueCardTone({ failed_count: 0, running_count: 1, pending_count: 1 }, 1),
    ).toBe("primary");
  });

  it("prioritizes pending over completed when no failed or running jobs", () => {
    expect(
      queueCardTone({ failed_count: 0, running_count: 0, pending_count: 1 }, 1),
    ).toBe("warning");
  });

  it("defaults completedCount to 0 when omitted (backwards-compat)", () => {
    expect(queueCardTone({ failed_count: 0, running_count: 0, pending_count: 0 })).toBeUndefined();
    expect(queueCardTone({ failed_count: 1, running_count: 0, pending_count: 0 })).toBe("danger");
  });
});

describe("levelCardTone", () => {
  it("returns undefined when live input is inactive regardless of current vs threshold", () => {
    expect(levelCardTone(false, -50, -60)).toBeUndefined();
    expect(levelCardTone(false, -30, -60)).toBeUndefined();
    expect(levelCardTone(false, -120, -60)).toBeUndefined();
  });

  it("returns undefined when current dBFS is below threshold", () => {
    expect(levelCardTone(true, -65, -60)).toBeUndefined();
    expect(levelCardTone(true, -120, -60)).toBeUndefined();
  });

  it("returns 'primary' when current dBFS is equal to threshold (gate edge)", () => {
    expect(levelCardTone(true, -60, -60)).toBe("primary");
  });

  it("returns 'primary' when current dBFS is above threshold (gate would open)", () => {
    expect(levelCardTone(true, -50, -60)).toBe("primary");
    expect(levelCardTone(true, -20, -60)).toBe("primary");
    expect(levelCardTone(true, 0, -60)).toBe("primary");
  });

  it("tracks threshold changes (active input, varying threshold)", () => {
    expect(levelCardTone(true, -50, -40)).toBeUndefined();
    expect(levelCardTone(true, -50, -50)).toBe("primary");
    expect(levelCardTone(true, -50, -60)).toBe("primary");
  });
});

describe("runtimeCardTone", () => {
  it("returns undefined when both microphone and runtime warnings are absent", () => {
    expect(runtimeCardTone(null, null)).toBeUndefined();
    expect(runtimeCardTone(undefined, undefined)).toBeUndefined();
    expect(runtimeCardTone(null, undefined)).toBeUndefined();
  });

  it("returns 'danger' when a microphone warning is present", () => {
    expect(runtimeCardTone("Pinned microphone is unavailable", null)).toBe("danger");
    expect(runtimeCardTone("Falling back to System Default", null)).toBe("danger");
  });

  it("returns 'danger' when the runtime warning is a live input stream error", () => {
    expect(
      runtimeCardTone(null, "Live input stream error: default input stream disconnected"),
    ).toBe("danger");
  });

  it("returns 'warning' for other recoverable runtime warnings", () => {
    expect(
      runtimeCardTone(null, "Live input dropped 42 stale audio frames while processing was busy"),
    ).toBe("warning");
  });

  it("prioritizes microphone warnings over runtime warnings", () => {
    expect(
      runtimeCardTone(
        "Pinned microphone is unavailable",
        "Live input stream error: default input stream disconnected",
      ),
    ).toBe("danger");
    expect(
      runtimeCardTone(
        "Pinned microphone is unavailable",
        "Live input dropped 42 stale audio frames while processing was busy",
      ),
    ).toBe("danger");
  });

  it("treats empty strings as absent (falsy) so a healthy runtime stays plain", () => {
    expect(runtimeCardTone("", "")).toBeUndefined();
    expect(runtimeCardTone("", null)).toBeUndefined();
  });
});

describe("trayStateBadgeTone", () => {
  it("maps idle to neutral (the default tray-state Badge tone)", () => {
    expect(trayStateBadgeTone("idle")).toBe("neutral");
  });

  it("maps listening to primary (capture monitoring is live)", () => {
    expect(trayStateBadgeTone("listening")).toBe("primary");
  });

  it("maps recording to success (speech gate has opened)", () => {
    expect(trayStateBadgeTone("recording")).toBe("success");
  });

  it("maps transcribing to warning (worker is busy processing the queue)", () => {
    expect(trayStateBadgeTone("transcribing")).toBe("warning");
  });

  it("maps paused to warning (operator-visible attention state)", () => {
    expect(trayStateBadgeTone("paused")).toBe("warning");
  });

  it("maps error to danger (needs attention)", () => {
    expect(trayStateBadgeTone("error")).toBe("danger");
  });

  it("falls back to neutral for unknown strings so the Badge contract stays total", () => {
    expect(trayStateBadgeTone("unknown")).toBe("neutral");
    expect(trayStateBadgeTone("")).toBe("neutral");
  });
});

describe("modelStatusBadgeTone", () => {
  it("maps ready to success (model is usable now)", () => {
    expect(modelStatusBadgeTone("ready")).toBe("success");
  });

  it("maps installed to success (model is usable now)", () => {
    expect(modelStatusBadgeTone("installed")).toBe("success");
  });

  it("maps downloading to primary (active in-flight)", () => {
    expect(modelStatusBadgeTone("downloading")).toBe("primary");
  });

  it("maps verifying to primary (active in-flight)", () => {
    expect(modelStatusBadgeTone("verifying")).toBe("primary");
  });

  it("maps extracting to primary (active in-flight)", () => {
    expect(modelStatusBadgeTone("extracting")).toBe("primary");
  });

  it("maps error to danger (needs attention)", () => {
    expect(modelStatusBadgeTone("error")).toBe("danger");
  });

  it("maps missing to neutral (no installation signal)", () => {
    expect(modelStatusBadgeTone("missing")).toBe("neutral");
  });

  it("maps unloaded to neutral (model is present but not in memory)", () => {
    expect(modelStatusBadgeTone("unloaded")).toBe("neutral");
  });

  it("falls back to neutral for unknown strings so the Badge contract stays total", () => {
    expect(modelStatusBadgeTone("unknown")).toBe("neutral");
    expect(modelStatusBadgeTone("")).toBe("neutral");
  });
});

describe("queueJobStatusBadgeTone", () => {
  it("maps running to primary (job is in flight)", () => {
    expect(queueJobStatusBadgeTone("running")).toBe("primary");
  });

  it("maps completed to success (happy-path throughput)", () => {
    expect(queueJobStatusBadgeTone("completed")).toBe("success");
  });

  it("maps failed to danger (needs retry)", () => {
    expect(queueJobStatusBadgeTone("failed")).toBe("danger");
  });

  it("maps cancelled to danger (needs retry to re-enter the queue)", () => {
    expect(queueJobStatusBadgeTone("cancelled")).toBe("danger");
  });

  it("maps skipped to warning (acknowledged but not done)", () => {
    expect(queueJobStatusBadgeTone("skipped")).toBe("warning");
  });

  it("maps pending to neutral (waiting, no signal)", () => {
    expect(queueJobStatusBadgeTone("pending")).toBe("neutral");
  });

  it("falls back to neutral for unknown strings so the Badge contract stays total", () => {
    expect(queueJobStatusBadgeTone("unknown")).toBe("neutral");
    expect(queueJobStatusBadgeTone("")).toBe("neutral");
  });
});

describe("queueStatsCellTone", () => {
  it("maps pending to warning (any backlog signals attention)", () => {
    expect(queueStatsCellTone("pending")).toBe("warning");
  });

  it("maps skipped to warning (acknowledged gap in the queue)", () => {
    expect(queueStatsCellTone("skipped")).toBe("warning");
  });

  it("maps running to primary (queue is in flight)", () => {
    expect(queueStatsCellTone("running")).toBe("primary");
  });

  it("maps failed to danger (needs retry)", () => {
    expect(queueStatsCellTone("failed")).toBe("danger");
  });

  it("maps cancelled to danger (needs retry to re-enter the queue)", () => {
    expect(queueStatsCellTone("cancelled")).toBe("danger");
  });

  it("maps completed to success (happy-path throughput)", () => {
    expect(queueStatsCellTone("completed")).toBe("success");
  });

  it("falls back to neutral for unknown strings so the call site stays total", () => {
    expect(queueStatsCellTone("unknown")).toBe("neutral");
    expect(queueStatsCellTone("")).toBe("neutral");
  });

  it("diverges from queueJobStatusBadgeTone for pending (warning vs neutral)", () => {
    // Per iter-73: the aggregate-bucket tone encodes "backlog → attention" while the
    // per-row Badge tone encodes "this single job is waiting → no signal". Pinning the
    // divergence here protects against a refactor that accidentally unifies them.
    expect(queueStatsCellTone("pending")).toBe("warning");
    expect(queueJobStatusBadgeTone("pending")).toBe("neutral");
  });
});
