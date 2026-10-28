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
  type StatusTone,
} from "./status-summary";
import { mockSnapshot } from "./app-state";
import type { TrayState } from "./types";

describe("capture status presentation", () => {
  it("describes stopped input with microphone and threshold context", () => {
    const snapshot = mockSnapshot();

    expect(captureStatusPresentation(snapshot)).toMatchObject({
      headline: "Input stopped",
      tone: "neutral",
      modeLabel: "Recording + transcription",
      microphone: "System Default",
      threshold: "-40 dBFS",
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

  it("maps each active tray_state to its capture headline and tone", () => {
    const snapshot = mockSnapshot();
    const cases: Array<[TrayState, { headline: string; tone: StatusTone }]> = [
      ["listening", { headline: "Listening for voice", tone: "primary" }],
      ["recording", { headline: "Recording speech", tone: "success" }],
      ["transcribing", { headline: "Transcribing queue", tone: "warning" }],
      ["paused", { headline: "Paused", tone: "warning" }],
      ["error", { headline: "Needs attention", tone: "danger" }],
    ];

    for (const [trayState, expected] of cases) {
      expect(
        captureStatusPresentation({
          ...snapshot,
          status: { ...snapshot.status, tray_state: trayState },
        }),
        trayState,
      ).toMatchObject(expected);
    }
  });

  it("classifies runtime warnings by severity", () => {
    const snapshot = mockSnapshot();
    const cases: Array<[string, StatusTone]> = [
      ["Live input dropped 42 stale audio frames while processing was busy", "warning"],
      ["Live input stream error: default input stream disconnected", "danger"],
    ];

    for (const [runtimeWarning, tone] of cases) {
      expect(
        captureStatusPresentation({
          ...snapshot,
          status: { ...snapshot.status, runtime_warning: runtimeWarning },
        }).warning,
        runtimeWarning,
      ).toEqual({
        key: `runtime:${runtimeWarning}`,
        message: runtimeWarning,
        tone,
      });
    }
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
    const jobs = [1, 2, 3].map((id) => ({
      id,
      audio_path: `/tmp/failed-${id}.wav`,
      model_id: "whisper-medium",
      status: "failed" as const,
    }));

    expect(
      captureStatusPresentation(
        {
          ...snapshot,
          queue: { ...snapshot.queue, jobs, failed_count: 3 },
          status: {
            ...snapshot.status,
            queue: { ...snapshot.queue, jobs, failed_count: 3 },
          },
        },
        {
          tone: "danger",
          count: 3,
          message: "3 transcription jobs need attention",
          nextExpiryAt: null,
        },
      ).warning,
    ).toEqual({
      key: "queue-danger:3",
      message: "3 transcription jobs need attention",
      tone: "danger",
    });
  });

  it("shows recent transcription quality attention in yellow", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation(snapshot, {
        tone: "warning",
        count: 1,
        message: "1 transcription warning needs review",
        nextExpiryAt: Date.parse("2026-08-04T00:10:00.000Z"),
      }).warning,
    ).toEqual({
      key: "queue-warning:1",
      message: "1 transcription warning needs review",
      tone: "warning",
    });
  });

  it("prioritizes Activity errors over recoverable runtime warnings", () => {
    const snapshot = mockSnapshot();

    expect(
      captureStatusPresentation(
        {
          ...snapshot,
          status: {
            ...snapshot.status,
            runtime_warning: "Live input dropped stale frames",
          },
        },
        {
          tone: "danger",
          count: 1,
          message: "1 transcription job needs attention",
          nextExpiryAt: null,
        },
      ).warning,
    ).toMatchObject({ key: "queue-danger:1", tone: "danger" });
  });

  it("clears the Capture warning after attention outcomes are resolved without deleting history", () => {
    const snapshot = mockSnapshot();
    const jobs = [
      {
        id: 1,
        audio_path: "/tmp/failed.wav",
        model_id: "whisper-medium",
        status: "failed" as const,
        is_read: true,
      },
      {
        id: 2,
        audio_path: "/tmp/cancelled.wav",
        model_id: "whisper-medium",
        status: "cancelled" as const,
        is_read: true,
      },
      {
        id: 3,
        audio_path: "/tmp/skipped.wav",
        model_id: "whisper-medium",
        status: "skipped" as const,
        is_read: true,
      },
    ];

    const presentation = captureStatusPresentation({
      ...snapshot,
      queue: { ...snapshot.queue, jobs, failed_count: 1 },
      status: {
        ...snapshot.status,
        queue: { ...snapshot.queue, jobs, failed_count: 1 },
      },
    });

    expect(presentation.warning).toBeNull();
    expect(presentation.queueSummary).toContain("1 failed");
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
  it("maps queue counts and attention to the queue card tone", () => {
    const dangerAttention = { tone: "danger" as const, count: 1, message: "error", nextExpiryAt: null };
    const cases: Array<[string, Parameters<typeof queueCardTone>, ReturnType<typeof queueCardTone>]> = [
      ["fully clean queue", [{ failed_count: 0, running_count: 0, pending_count: 0 }, 0], undefined],
      [
        "danger attention with clean counts",
        [{ failed_count: 0, running_count: 0, pending_count: 0 }, 0, dangerAttention],
        "danger",
      ],
      ["raw failed count alone", [{ failed_count: 42, running_count: 0, pending_count: 0 }, 0], undefined],
      ["one running", [{ failed_count: 0, running_count: 1, pending_count: 0 }, 0], "primary"],
      ["running with pending", [{ failed_count: 0, running_count: 3, pending_count: 5 }, 0], "primary"],
      ["one pending", [{ failed_count: 0, running_count: 0, pending_count: 1 }, 0], "warning"],
      ["several pending", [{ failed_count: 0, running_count: 0, pending_count: 8 }, 0], "warning"],
      ["one completed", [{ failed_count: 0, running_count: 0, pending_count: 0 }, 1], "success"],
      ["several completed", [{ failed_count: 0, running_count: 0, pending_count: 0 }, 17], "success"],
      [
        "danger attention over running, pending, and completed",
        [{ failed_count: 1, running_count: 1, pending_count: 1 }, 1, dangerAttention],
        "danger",
      ],
      ["running over pending and completed", [{ failed_count: 0, running_count: 1, pending_count: 1 }, 1], "primary"],
      ["pending over completed", [{ failed_count: 0, running_count: 0, pending_count: 1 }, 1], "warning"],
      ["omitted completedCount on a clean queue", [{ failed_count: 0, running_count: 0, pending_count: 0 }], undefined],
      ["omitted completedCount with a failed job", [{ failed_count: 1, running_count: 0, pending_count: 0 }], undefined],
    ];

    for (const [label, args, expected] of cases) {
      expect(queueCardTone(...args), label).toBe(expected);
    }
  });
});

describe("levelCardTone", () => {
  it("maps live input level against threshold to the level card tone", () => {
    const cases: Array<[boolean, number, number, ReturnType<typeof levelCardTone>]> = [
      // Inactive input stays plain regardless of current vs threshold.
      [false, -50, -60, undefined],
      [false, -30, -60, undefined],
      [false, -120, -60, undefined],
      // Below threshold.
      [true, -65, -60, undefined],
      [true, -120, -60, undefined],
      // Equal to threshold is the gate edge.
      [true, -60, -60, "primary"],
      // Above threshold (gate would open).
      [true, -50, -60, "primary"],
      [true, -20, -60, "primary"],
      [true, 0, -60, "primary"],
      // Tracks threshold changes.
      [true, -50, -40, undefined],
      [true, -50, -50, "primary"],
      [true, -50, -60, "primary"],
    ];

    for (const [active, current, threshold, expected] of cases) {
      expect(levelCardTone(active, current, threshold), `${active} ${current}/${threshold}`).toBe(expected);
    }
  });
});

describe("runtimeCardTone", () => {
  it("maps microphone and runtime warnings to the runtime card tone", () => {
    const cases: Array<[string | null | undefined, string | null | undefined, ReturnType<typeof runtimeCardTone>]> = [
      [null, null, undefined],
      [undefined, undefined, undefined],
      [null, undefined, undefined],
      // Empty strings are absent (falsy) so a healthy runtime stays plain.
      ["", "", undefined],
      ["", null, undefined],
      ["Pinned microphone is unavailable", null, "danger"],
      ["Falling back to System Default", null, "danger"],
      [null, "Live input stream error: default input stream disconnected", "danger"],
      [null, "Live input dropped 42 stale audio frames while processing was busy", "warning"],
      // Microphone warnings win over runtime warnings.
      ["Pinned microphone is unavailable", "Live input stream error: default input stream disconnected", "danger"],
      ["Pinned microphone is unavailable", "Live input dropped 42 stale audio frames while processing was busy", "danger"],
    ];

    for (const [microphone, runtime, expected] of cases) {
      expect(runtimeCardTone(microphone, runtime), `${String(microphone)} / ${String(runtime)}`).toBe(expected);
    }
  });
});

describe("trayStateBadgeTone", () => {
  it("maps every tray_state to its Badge tone", () => {
    const cases: Array<[string, StatusTone]> = [
      ["idle", "neutral"],
      ["listening", "primary"],
      ["recording", "success"],
      ["transcribing", "warning"],
      ["paused", "warning"],
      ["error", "danger"],
      // Unknown strings fall back to neutral so the Badge contract stays total.
      ["unknown", "neutral"],
      ["", "neutral"],
    ];

    for (const [state, tone] of cases) {
      expect(trayStateBadgeTone(state), state).toBe(tone);
    }
  });
});

describe("modelStatusBadgeTone", () => {
  it("maps every model status to its Badge tone", () => {
    const cases: Array<[string, StatusTone]> = [
      ["ready", "success"],
      ["installed", "success"],
      ["downloading", "primary"],
      ["verifying", "primary"],
      ["extracting", "primary"],
      ["error", "danger"],
      ["missing", "neutral"],
      ["unloaded", "neutral"],
      // Unknown strings fall back to neutral so the Badge contract stays total.
      ["unknown", "neutral"],
      ["", "neutral"],
    ];

    for (const [status, tone] of cases) {
      expect(modelStatusBadgeTone(status), status).toBe(tone);
    }
  });
});

describe("queueJobStatusBadgeTone", () => {
  it("maps every queue job status to its Badge tone", () => {
    const cases: Array<[string, StatusTone]> = [
      ["running", "primary"],
      ["completed", "success"],
      ["failed", "danger"],
      ["cancelled", "warning"],
      ["skipped", "warning"],
      ["pending", "neutral"],
      // Unknown strings fall back to neutral so the Badge contract stays total.
      ["unknown", "neutral"],
      ["", "neutral"],
    ];

    for (const [status, tone] of cases) {
      expect(queueJobStatusBadgeTone(status), status).toBe(tone);
    }
  });
});

describe("queueStatsCellTone", () => {
  it("maps every queue stats bucket to its cell tone", () => {
    const cases: Array<[string, StatusTone]> = [
      // Per iter-73: the aggregate-bucket tone encodes "backlog → attention" while the
      // per-row Badge tone encodes "this single job is waiting → no signal" (pending → neutral
      // in queueJobStatusBadgeTone). Pinning the divergence here protects against a refactor
      // that accidentally unifies them.
      ["pending", "warning"],
      ["skipped", "warning"],
      ["running", "primary"],
      ["failed", "danger"],
      ["cancelled", "warning"],
      ["warning", "warning"],
      ["error", "danger"],
      ["completed", "success"],
      // Unknown strings fall back to neutral so the call site stays total.
      ["unknown", "neutral"],
      ["", "neutral"],
    ];

    for (const [status, tone] of cases) {
      expect(queueStatsCellTone(status), status).toBe(tone);
    }
  });
});

describe("activeWarning with silence_warning", () => {
  it("emits a warning banner when silence_warning is present", () => {
    const snapshot = mockSnapshot();
    const presentation = captureStatusPresentation({
      ...snapshot,
      status: {
        ...snapshot.status,
        silence_warning: { device_label: "Mic A", seconds: 14 },
      },
    });
    expect(presentation.warning).toEqual({
      key: "silence:Mic A:14",
      message: "No input detected from microphone 'Mic A' for 14s. Check your microphone settings.",
      tone: "warning",
    });
  });

  it("prefers microphone_warning over silence_warning when both are set", () => {
    const snapshot = mockSnapshot();
    const presentation = captureStatusPresentation({
      ...snapshot,
      status: {
        ...snapshot.status,
        microphone_warning: "Mic fallback in use",
        silence_warning: { device_label: "Mic A", seconds: 14 },
      },
    });
    expect(presentation.warning?.key).toMatch(/^microphone:/);
  });

  it("clears the silence warning when silence_warning is null", () => {
    const snapshot = mockSnapshot();
    const presentation = captureStatusPresentation({
      ...snapshot,
      status: { ...snapshot.status, silence_warning: null },
    });
    expect(presentation.warning).toBeNull();
  });
});
