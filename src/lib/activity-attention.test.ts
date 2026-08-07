import { describe, expect, it } from "vitest";
import {
  ACTIVITY_WARNING_WINDOW_MS,
  activityAttentionAt,
  activityIssue,
  activityIssueCounts,
  activityJobPresentation,
  countUnreadActivityOutcomes,
  isReprocessableActivityJob,
} from "./activity-attention";
import type { QueueJob, QueueJobIssue } from "./types";

const now = Date.parse("2026-08-04T12:00:00.000Z");

function job(
  status: QueueJob["status"],
  issue?: QueueJobIssue,
  overrides: Partial<QueueJob> = {},
): QueueJob {
  return {
    id: 1,
    audio_path: "/tmp/audio.wav",
    model_id: "whisper-medium",
    status,
    issue,
    ...overrides,
  };
}

function warning(occurredAt: number | null = now): QueueJobIssue {
  return {
    severity: "warning",
    code: "empty_transcript",
    message: "OpenAI returned an empty transcript",
    occurred_at: occurredAt === null ? null : new Date(occurredAt).toISOString(),
  };
}

function error(): QueueJobIssue {
  return {
    severity: "error",
    code: "provider",
    message: "Provider unavailable",
    occurred_at: new Date(now - 30_000).toISOString(),
  };
}

describe("activity attention", () => {
  it("keeps a warning active through 599999 ms and expires it at 600000 ms", () => {
    const warningJob = job("completed", warning(now));

    expect(activityAttentionAt([warningJob], now + ACTIVITY_WARNING_WINDOW_MS - 1)).toMatchObject({
      tone: "warning",
      count: 1,
      nextExpiryAt: now + ACTIVITY_WARNING_WINDOW_MS,
    });
    expect(activityAttentionAt([warningJob], now + ACTIVITY_WARNING_WINDOW_MS)).toBeNull();
  });

  it("keeps unread errors active and gives them priority over recent warnings", () => {
    expect(
      activityAttentionAt(
        [job("completed", warning()), job("failed", error(), { id: 2 })],
        now,
      ),
    ).toEqual({
      tone: "danger",
      count: 1,
      message: "1 transcription job needs attention",
      nextExpiryAt: null,
    });
  });

  it("clears resolved outcomes immediately", () => {
    expect(
      activityAttentionAt([job("failed", error(), { is_read: true })], now),
    ).toBeNull();
  });

  it("does not restart warning timers with missing, legacy, or invalid timestamps", () => {
    expect(activityAttentionAt([job("completed", warning(null))], now)).toBeNull();
    expect(
      activityAttentionAt(
        [job("completed", { ...warning(), occurred_at: "not-a-date" })],
        now,
      ),
    ).toBeNull();
    expect(
      activityAttentionAt(
        [
          job("failed", undefined, {
            error: "OpenAI returned an empty transcript",
          }),
        ],
        now,
      ),
    ).toBeNull();
  });

  it("classifies legacy empty and no-speech failures as warnings", () => {
    expect(
      activityIssue(
        job("failed", undefined, { error: "OpenAI returned an empty transcript" }),
      ),
    ).toMatchObject({ severity: "warning", code: "empty_transcript" });
    expect(
      activityIssue(job("failed", undefined, { error: "No speech detected" })),
    ).toMatchObject({ severity: "warning", code: "no_speech" });
  });

  it("keeps completed warnings in attention and all completions reprocessable", () => {
    const completedWarning = job("completed", warning());
    const cleanCompleted = job("completed", undefined, { id: 2 });

    expect(countUnreadActivityOutcomes([completedWarning, cleanCompleted])).toBe(1);
    expect(isReprocessableActivityJob(completedWarning)).toBe(true);
    expect(isReprocessableActivityJob(cleanCompleted)).toBe(true);
    expect(activityIssueCounts([completedWarning, job("failed", error(), { id: 3 })])).toEqual({
      warning: 1,
      error: 1,
    });
  });

  it("shows completed artifacts as Completed plus Warning", () => {
    expect(activityJobPresentation(job("completed", warning()))).toEqual({
      primaryLabel: "Completed",
      primaryTone: "success",
      issueLabel: "Warning",
      issueTone: "warning",
      message: "OpenAI returned an empty transcript",
    });
  });
});
