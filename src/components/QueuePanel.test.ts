import { describe, expect, it } from "vitest";
import {
  activityPage,
  cancelCurrentDisabledReason,
  filterActivityJobs,
  isReprocessableJob,
  preferredReprocessingModelId,
  processNextDisabledReason,
  queueJobActionState,
  queueJobRetryDisabledReason,
  queueJobSkipDisabledReason,
  queueToolbarActionState,
  reprocessingModels,
} from "./QueuePanel";
import type { ModelDescriptor, QueueJob, QueueJobStatus } from "../lib/types";

function model(
  id: string,
  status: ModelDescriptor["status"],
  selectableContexts: ModelDescriptor["capabilities"]["selectable_contexts"],
): ModelDescriptor {
  return {
    id,
    display_name: id,
    engine: "test",
    provider_runtime: "test",
    size_mb: 1,
    languages: ["en"],
    speed_score: 1,
    accuracy_score: 1,
    offline: true,
    status,
    capabilities: {
      file_transcription: selectableContexts.includes("file"),
      realtime: selectableContexts.includes("realtime"),
      streaming: "unsupported",
      diarization: false,
      cost_reporting: "none",
      maximum_request_bytes: null,
      selectable_contexts: selectableContexts,
    },
  };
}

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

    expect(first).toMatchObject({
      page: 1,
      pageCount: 2,
      rangeStart: 1,
      rangeEnd: 50,
      total: 51,
    });
    expect(first.jobs).toHaveLength(50);
    expect(first.jobs[0]?.id).toBe(51);
    expect(second).toMatchObject({
      page: 2,
      pageCount: 2,
      rangeStart: 51,
      rangeEnd: 51,
      total: 51,
    });
    expect(second.jobs.map((job) => job.id)).toEqual([1]);

    const oldestFirst = activityPage(
      [...olderJobs, newestJob],
      1,
      50,
      "oldest",
    );
    expect(oldestFirst.jobs[0]?.id).toBe(1);
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
    {
      id: 2,
      audio_path: "/resolved-failed.wav",
      model_id: "m",
      status: "failed",
      is_read: true,
    },
    {
      id: 3,
      audio_path: "/resolved-skipped.wav",
      model_id: "m",
      status: "skipped",
      is_read: true,
    },
    { id: 4, audio_path: "/completed.wav", model_id: "m", status: "completed" },
  ];

  it("keeps resolved outcomes discoverable after they stop requiring attention", () => {
    expect(filterActivityJobs(jobs, "resolved").map((job) => job.id)).toEqual([
      2, 3,
    ]);
    expect(filterActivityJobs(jobs, "attention").map((job) => job.id)).toEqual([
      1,
    ]);
    expect(filterActivityJobs(jobs, "all").map((job) => job.id)).toEqual([
      1, 2, 3,
    ]);
    expect(
      filterActivityJobs(jobs, "all", {
        modelId: "all",
        kind: "all",
        status: "all",
      }).map((job) => job.id),
    ).toEqual([1, 2, 3, 4]);
  });

  it("gathers warnings and errors from every read state into one review list", () => {
    const mixed: QueueJob[] = [
      { id: 1, audio_path: "/pending.wav", model_id: "m", status: "pending" },
      { id: 2, audio_path: "/failed.wav", model_id: "m", status: "failed" },
      {
        id: 3,
        audio_path: "/resolved-cancelled.wav",
        model_id: "m",
        status: "cancelled",
        is_read: true,
      },
      { id: 4, audio_path: "/clean.wav", model_id: "m", status: "completed" },
      {
        id: 5,
        audio_path: "/warned.wav",
        model_id: "m",
        status: "completed",
        issue: {
          severity: "warning",
          code: "no_speech",
          message: "No speech detected",
        },
      },
    ];

    expect(filterActivityJobs(mixed, "issues").map((job) => job.id)).toEqual([
      2, 3, 5,
    ]);
    expect(filterActivityJobs(mixed, "all").map((job) => job.id)).toEqual([
      1, 2, 3, 5,
    ]);
  });

  it("combines model, Dictation/live type, and status filters", () => {
    const candidates: QueueJob[] = [
      {
        id: 1,
        audio_path: "/dictation.wav",
        model_id: "openai",
        status: "failed",
        activity_kind: "dictation",
      },
      {
        id: 2,
        audio_path: "/live.wav",
        model_id: "openai",
        status: "failed",
        activity_kind: "live_transcription",
      },
      {
        id: 3,
        audio_path: "/done.wav",
        model_id: "local",
        status: "completed",
        activity_kind: "live_transcription",
      },
    ];

    expect(
      filterActivityJobs(candidates, "all", {
        modelId: "openai",
        kind: "dictation",
        status: "failed",
      }).map((job) => job.id),
    ).toEqual([1]);
  });
});

describe("Activity issue reprocessing", () => {
  it("marks finished jobs reprocessable and keeps completed warnings in attention", () => {
    const completedWarning: QueueJob = {
      id: 1,
      audio_path: "/warning.wav",
      model_id: "old",
      status: "completed",
      issue: {
        severity: "warning",
        code: "transcript_artifact",
        message: "Transcript may contain an artifact",
        occurred_at: "2026-08-04T12:00:00.000Z",
      },
    };
    const cleanCompleted: QueueJob = {
      ...completedWarning,
      id: 2,
      issue: null,
    };
    const statusJob = (status: QueueJobStatus): QueueJob => ({
      id: 1,
      audio_path: "/recording.wav",
      model_id: "old",
      status,
    });
    const cases: Array<[string, QueueJob, boolean]> = [
      ["failed", statusJob("failed"), true],
      ["cancelled", statusJob("cancelled"), true],
      ["skipped", statusJob("skipped"), true],
      ["pending", statusJob("pending"), false],
      ["running", statusJob("running"), false],
      ["completed", statusJob("completed"), true],
      ["completed with warning", completedWarning, true],
      ["completed with null issue", cleanCompleted, true],
    ];

    for (const [label, job, expected] of cases) {
      expect(isReprocessableJob(job), label).toBe(expected);
    }
    expect(filterActivityJobs([completedWarning], "attention")).toEqual([
      completedWarning,
    ]);
  });

  it("offers runnable reprocessing models and falls back to the current model without Soniox Async", () => {
    const models = [
      model("ready-file", "ready", ["file"]),
      model("installed-file", "installed", ["file"]),
      model("unloaded-file", "unloaded", ["file"]),
      model("missing-file", "missing", ["file"]),
      model("ready-realtime", "ready", ["realtime"]),
    ];

    expect(reprocessingModels(models).map((candidate) => candidate.id)).toEqual(
      ["ready-file", "installed-file", "unloaded-file"],
    );
    expect(preferredReprocessingModelId(models, "installed-file")).toBe(
      "installed-file",
    );
    expect(preferredReprocessingModelId(models, "missing-file")).toBe(
      "ready-file",
    );
  });

  it("defaults to Soniox Async when available regardless of the global model", () => {
    const models = [
      model("ready-file", "ready", ["file"]),
      model("soniox-async-v5", "ready", ["file"]),
    ];

    expect(preferredReprocessingModelId(models, "ready-file")).toBe(
      "soniox-async-v5",
    );
    expect(
      preferredReprocessingModelId(
        models.map((candidate) => ({ ...candidate, status: "missing" })),
        "ready-file",
      ),
    ).toBe("");
  });
});

describe("queue row actions", () => {
  it("derives row Retry/Skip availability and disabled reasons for every status", () => {
    const cases: Array<{
      status: QueueJobStatus;
      canRetry: boolean;
      canSkip: boolean;
      retryReason: string | null;
      skipReason: string | null;
    }> = [
      {
        status: "pending",
        canRetry: false,
        canSkip: true,
        retryReason: "Job has not run yet",
        skipReason: null,
      },
      {
        status: "running",
        canRetry: false,
        canSkip: false,
        retryReason: "Job is still running",
        skipReason: "Job is still running",
      },
      {
        status: "completed",
        canRetry: true,
        canSkip: false,
        retryReason: null,
        skipReason: "Job already completed",
      },
      {
        status: "failed",
        canRetry: true,
        canSkip: true,
        retryReason: null,
        skipReason: null,
      },
      {
        status: "cancelled",
        canRetry: true,
        canSkip: true,
        retryReason: null,
        skipReason: null,
      },
      {
        status: "skipped",
        canRetry: true,
        canSkip: false,
        retryReason: null,
        skipReason: "Job already skipped",
      },
    ];

    for (const { status, canRetry, canSkip, retryReason, skipReason } of cases) {
      expect(queueJobActionState(status), `${status} actions`).toEqual({
        canRetry,
        canSkip,
      });
      expect(queueJobRetryDisabledReason(status), `${status} retry`).toBe(
        retryReason,
      );
      expect(queueJobSkipDisabledReason(status), `${status} skip`).toBe(
        skipReason,
      );
    }
  });
});

describe("queue toolbar actions", () => {
  it("derives toolbar Process next/Cancel current availability and reasons", () => {
    const cases: Array<{
      pending_count: number;
      running_count: number;
      canProcess: boolean;
      canProcessNext: boolean;
      canCancelCurrent: boolean;
      processReason: string | null;
      cancelReason: string | null;
    }> = [
      {
        pending_count: 1,
        running_count: 0,
        canProcess: true,
        canProcessNext: true,
        canCancelCurrent: false,
        processReason: null,
        cancelReason: "No running job to cancel",
      },
      {
        pending_count: 1,
        running_count: 0,
        canProcess: false,
        canProcessNext: false,
        canCancelCurrent: false,
        processReason: "Transcription unavailable",
        cancelReason: "No running job to cancel",
      },
      {
        pending_count: 0,
        running_count: 1,
        canProcess: true,
        canProcessNext: false,
        canCancelCurrent: true,
        processReason: "A job is already running",
        cancelReason: null,
      },
      {
        pending_count: 1,
        running_count: 1,
        canProcess: true,
        canProcessNext: false,
        canCancelCurrent: true,
        processReason: "A job is already running",
        cancelReason: null,
      },
      // An in-flight job takes precedence over any other disabled reason.
      {
        pending_count: 1,
        running_count: 1,
        canProcess: false,
        canProcessNext: false,
        canCancelCurrent: true,
        processReason: "A job is already running",
        cancelReason: null,
      },
      {
        pending_count: 0,
        running_count: 0,
        canProcess: true,
        canProcessNext: false,
        canCancelCurrent: false,
        processReason: "No pending jobs",
        cancelReason: "No running job to cancel",
      },
    ];

    for (const {
      pending_count,
      running_count,
      canProcess,
      canProcessNext,
      canCancelCurrent,
      processReason,
      cancelReason,
    } of cases) {
      const label = `pending=${pending_count} running=${running_count} canProcess=${canProcess}`;
      const queue = { jobs: [], pending_count, running_count, failed_count: 0 };

      expect(queueToolbarActionState(queue, canProcess), label).toEqual({
        canProcessNext,
        canCancelCurrent,
      });
      expect(processNextDisabledReason(queue, canProcess), label).toBe(
        processReason,
      );
      expect(cancelCurrentDisabledReason(queue), label).toBe(cancelReason);
    }
  });
});
