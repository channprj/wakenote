import { describe, expect, it } from "vitest";
import {
  cancelCurrentDisabledReason,
  processNextDisabledReason,
  queueJobActionState,
  queueJobRetryDisabledReason,
  queueJobSkipDisabledReason,
  queueToolbarActionState,
  toneForStatus,
} from "./QueuePanel";
import type { QueueJobStatus } from "../lib/types";

describe("queue row actions", () => {
  it.each([
    ["pending", false, true],
    ["running", false, false],
    ["completed", false, false],
    ["failed", true, true],
    ["cancelled", true, true],
    ["skipped", false, false],
  ] satisfies Array<[QueueJobStatus, boolean, boolean]>)(
    "derives valid actions for %s jobs",
    (status, canRetry, canSkip) => {
      expect(queueJobActionState(status)).toEqual({ canRetry, canSkip });
    },
  );
});

describe("queue toolbar actions", () => {
  it("enables process next only when pending work can be transcribed", () => {
    const queue = {
      jobs: [],
      pending_count: 1,
      running_count: 0,
      failed_count: 0,
    };

    expect(queueToolbarActionState(queue, true)).toEqual({
      canProcessNext: true,
      canCancelCurrent: false,
    });
    expect(queueToolbarActionState(queue, false)).toEqual({
      canProcessNext: false,
      canCancelCurrent: false,
    });
  });

  it("enables cancel current only while a queue job is running", () => {
    const queue = {
      jobs: [],
      pending_count: 0,
      running_count: 1,
      failed_count: 0,
    };

    expect(queueToolbarActionState(queue, true)).toEqual({
      canProcessNext: false,
      canCancelCurrent: true,
    });
  });

  it("disables process next while a queue job is already running", () => {
    const queue = {
      jobs: [],
      pending_count: 1,
      running_count: 1,
      failed_count: 0,
    };

    expect(queueToolbarActionState(queue, true)).toEqual({
      canProcessNext: false,
      canCancelCurrent: true,
    });
  });
});

describe("process next disabled reason", () => {
  it("returns null while process next is operable", () => {
    expect(
      processNextDisabledReason({ pending_count: 1, running_count: 0 }, true),
    ).toBeNull();
  });

  it("flags an in-flight job ahead of any other disabled reason", () => {
    expect(
      processNextDisabledReason({ pending_count: 1, running_count: 1 }, false),
    ).toBe("A job is already running");
  });

  it("falls back to an empty-queue message when nothing is pending", () => {
    expect(
      processNextDisabledReason({ pending_count: 0, running_count: 0 }, true),
    ).toBe("No pending jobs");
  });

  it("blames the transcription pipeline when pending work cannot be processed", () => {
    expect(
      processNextDisabledReason({ pending_count: 1, running_count: 0 }, false),
    ).toBe("Transcription unavailable");
  });
});

describe("cancel current disabled reason", () => {
  it("returns null while a job is running", () => {
    expect(cancelCurrentDisabledReason({ running_count: 1 })).toBeNull();
  });

  it("returns an empty-runner message when nothing is in flight", () => {
    expect(cancelCurrentDisabledReason({ running_count: 0 })).toBe(
      "No running job to cancel",
    );
  });
});

describe("queue job retry disabled reason", () => {
  it.each([
    ["failed", null],
    ["cancelled", null],
    ["pending", "Job has not run yet"],
    ["running", "Job is still running"],
    ["completed", "Job already completed"],
    ["skipped", "Job was skipped"],
  ] satisfies Array<[QueueJobStatus, string | null]>)(
    "describes retry availability for %s jobs",
    (status, reason) => {
      expect(queueJobRetryDisabledReason(status)).toBe(reason);
    },
  );
});

describe("tone for status", () => {
  it.each([
    ["pending", "neutral"],
    ["running", "primary"],
    ["completed", "success"],
    ["failed", "danger"],
    ["cancelled", "danger"],
    ["skipped", "warning"],
  ] satisfies Array<[QueueJobStatus, string]>)(
    "maps %s jobs to the %s tone",
    (status, tone) => {
      expect(toneForStatus(status)).toBe(tone);
    },
  );
});

describe("queue job skip disabled reason", () => {
  it.each([
    ["pending", null],
    ["failed", null],
    ["cancelled", null],
    ["running", "Job is still running"],
    ["completed", "Job already completed"],
    ["skipped", "Job already skipped"],
  ] satisfies Array<[QueueJobStatus, string | null]>)(
    "describes skip availability for %s jobs",
    (status, reason) => {
      expect(queueJobSkipDisabledReason(status)).toBe(reason);
    },
  );
});
