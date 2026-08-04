import type {
  QueueIssueCode,
  QueueJob,
  QueueJobIssue,
  QueueJobStatus,
} from "./types";

export const ACTIVITY_WARNING_WINDOW_MS = 10 * 60 * 1_000;

export type ActivityTone =
  | "neutral"
  | "success"
  | "warning"
  | "danger"
  | "primary";

export interface ActivityAttention {
  tone: "warning" | "danger";
  count: number;
  message: string;
  nextExpiryAt: number | null;
}

export interface ActivityJobPresentation {
  primaryLabel: string;
  primaryTone: ActivityTone;
  issueLabel: string | null;
  issueTone: "warning" | "danger" | null;
  message: string | null;
}

const statusPresentation: Record<
  QueueJobStatus,
  Pick<ActivityJobPresentation, "primaryLabel" | "primaryTone">
> = {
  pending: { primaryLabel: "Pending", primaryTone: "neutral" },
  running: { primaryLabel: "Running", primaryTone: "primary" },
  completed: { primaryLabel: "Completed", primaryTone: "success" },
  failed: { primaryLabel: "Failed", primaryTone: "danger" },
  cancelled: { primaryLabel: "Cancelled", primaryTone: "warning" },
  skipped: { primaryLabel: "Skipped", primaryTone: "warning" },
};

function legacyIssue(
  code: QueueIssueCode,
  message: string,
  severity: "warning" | "error",
): QueueJobIssue {
  return { severity, code, message, occurred_at: null };
}

export function activityIssue(job: QueueJob): QueueJobIssue | null {
  if (job.issue) {
    return job.issue;
  }

  if (job.status === "failed") {
    const message = job.error?.trim() || "Transcription failed";
    if (message === "No speech detected") {
      return legacyIssue("no_speech", message, "warning");
    }
    if (message.endsWith("returned an empty transcript")) {
      return legacyIssue("empty_transcript", message, "warning");
    }
    return legacyIssue("unknown", message, "error");
  }

  if (job.status === "cancelled") {
    return legacyIssue("cancelled", job.error?.trim() || "Cancelled", "warning");
  }
  if (job.status === "skipped") {
    return legacyIssue("skipped", job.error?.trim() || "Skipped", "warning");
  }

  return null;
}

export function isActivityAttentionOutcome(job: QueueJob): boolean {
  if (["failed", "cancelled", "skipped"].includes(job.status)) {
    return true;
  }
  return job.status === "completed" && activityIssue(job)?.severity === "warning";
}

export function isReprocessableActivityJob(job: QueueJob): boolean {
  return isActivityAttentionOutcome(job);
}

export function isRecentUnreadWarning(job: QueueJob, nowMs: number): boolean {
  const issue = activityIssue(job);
  if (job.is_read === true || issue?.severity !== "warning") {
    return false;
  }

  const occurredAt = Date.parse(issue.occurred_at ?? "");
  if (!Number.isFinite(occurredAt)) {
    return false;
  }
  const age = nowMs - occurredAt;
  return age >= 0 && age < ACTIVITY_WARNING_WINDOW_MS;
}

export function activityAttentionAt(
  jobs: QueueJob[],
  nowMs: number,
): ActivityAttention | null {
  const unreadIssues = jobs
    .filter((job) => job.is_read !== true && isActivityAttentionOutcome(job))
    .map((job) => ({ job, issue: activityIssue(job) }))
    .filter(
      (entry): entry is { job: QueueJob; issue: QueueJobIssue } =>
        entry.issue !== null,
    );
  const errorCount = unreadIssues.filter(
    ({ issue }) => issue.severity === "error",
  ).length;
  if (errorCount > 0) {
    return {
      tone: "danger",
      count: errorCount,
      message: `${errorCount} transcription ${errorCount === 1 ? "job needs" : "jobs need"} attention`,
      nextExpiryAt: null,
    };
  }

  const warnings = unreadIssues.filter(({ job }) =>
    isRecentUnreadWarning(job, nowMs),
  );
  if (warnings.length === 0) {
    return null;
  }

  const expiries = warnings
    .map(({ issue }) => Date.parse(issue.occurred_at ?? "") + ACTIVITY_WARNING_WINDOW_MS)
    .filter(Number.isFinite);
  return {
    tone: "warning",
    count: warnings.length,
    message: `${warnings.length} transcription ${warnings.length === 1 ? "warning needs" : "warnings need"} review`,
    nextExpiryAt: Math.min(...expiries),
  };
}

export function countUnreadActivityOutcomes(jobs: QueueJob[]): number {
  return jobs.filter(
    (job) => job.is_read !== true && isActivityAttentionOutcome(job),
  ).length;
}

export function activityIssueCounts(jobs: QueueJob[]): {
  warning: number;
  error: number;
} {
  return jobs.reduce(
    (counts, job) => {
      if (!isActivityAttentionOutcome(job)) {
        return counts;
      }
      const issue = activityIssue(job);
      if (issue) {
        counts[issue.severity] += 1;
      }
      return counts;
    },
    { warning: 0, error: 0 },
  );
}

export function activityJobPresentation(job: QueueJob): ActivityJobPresentation {
  const issue = activityIssue(job);
  const base = statusPresentation[job.status];
  if (!issue) {
    return { ...base, issueLabel: null, issueTone: null, message: job.error ?? null };
  }

  if (job.status === "completed" && issue.severity === "warning") {
    return {
      ...base,
      issueLabel: "Warning",
      issueTone: "warning",
      message: issue.message,
    };
  }

  return {
    primaryLabel: issue.severity === "warning" ? "Warning" : "Failed",
    primaryTone: issue.severity === "warning" ? "warning" : "danger",
    issueLabel: null,
    issueTone: null,
    message: issue.message,
  };
}
