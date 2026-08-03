import { describe, expect, it } from "vitest";
import {
  activityPage,
  cancelCurrentDisabledReason,
  filterActivityJobs,
  processNextDisabledReason,
  queueJobActionState,
  queueJobRetryDisabledReason,
  queueJobSkipDisabledReason,
  queueToolbarActionState,
} from "./QueuePanel";
import type { QueueJob, QueueJobStatus } from "../lib/types";

describe("Activity pagination", () => {
  it("shows the newest day first and caps each page at 50 jobs", () => {
    const olderJobs: QueueJob[] = Array.from({ length: 50 }, (_, index) => ({
      id: index + 1,
      audio_path: `/recordings/20260802/${String(index + 1).padStart(6, "0")}.wav`,
      model_id: "whisper-medium",
      status: "completed",
    }));
    const newestJob: QueueJob = {
      id: 51,
      audio_path: "/recordings/20260803/010203.wav",
      model_id: "whisper-medium",
      status: "failed",
    };

    const first = activityPage([...olderJobs, newestJob], 1);
    const second = activityPage([...olderJobs, newestJob], 2);

    expect(first).toMatchObject({ page: 1, pageCount: 2, rangeStart: 1, rangeEnd: 50, total: 51 });
    expect(first.jobs).toHaveLength(50);
    expect(first.jobs[0]?.id).toBe(51);
    expect(second).toMatchObject({ page: 2, pageCount: 2, rangeStart: 51, rangeEnd: 51, total: 51 });
    expect(second.jobs.map((job) => job.id)).toEqual([50]);
  });

  it("clamps out-of-range pages and keeps an empty queue on page one", () => {
    expect(activityPage([], 7)).toMatchObject({
      jobs: [],
      page: 1,
      pageCount: 1,
      rangeStart: 0,
      rangeEnd: 0,
      total: 0,
    });
  });
});

describe("Activity issue views", () => {
  const jobs: QueueJob[] = [
    { id: 1, audio_path: "/unread.wav", model_id: "m", status: "failed" },
    { id: 2, audio_path: "/resolved-failed.wav", model_id: "m", status: "failed", is_read: true },
    { id: 3, audio_path: "/resolved-skipped.wav", model_id: "m", status: "skipped", is_read: true },
    { id: 4, audio_path: "/completed.wav", model_id: "m", status: "completed" },
  ];

  it("keeps resolved outcomes discoverable after they stop requiring attention", () => {
    expect(filterActivityJobs(jobs, "resolved").map((job) => job.id)).toEqual([2, 3]);
    expect(filterActivityJobs(jobs, "attention").map((job) => job.id)).toEqual([1]);
    expect(filterActivityJobs(jobs, "all").map((job) => job.id)).toEqual([1, 2, 3, 4]);
  });
});

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
