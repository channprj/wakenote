import { Ban, FileAudio, FolderInput, Play, RotateCw, SkipForward } from "lucide-react";
import { Fragment } from "react";
import { Badge, Button } from "./ui/primitives";
import { formatModelLabel } from "../lib/models";
import { queueJobStatusBadgeTone, queueStatsCellTone } from "../lib/status-summary";
import {
  countCancelledQueueJobs,
  countCompletedQueueJobs,
  countFailedQueueJobs,
  countPendingQueueJobs,
  countRunningQueueJobs,
  countSkippedQueueJobs,
  fileUrlFromPath,
  formatAudioPathLabel,
  groupQueueJobsByDay,
  humanizeQueueJobStatus,
  queueJobSidecarPath,
  summarizeQueueJobsByDay,
} from "../lib/transcript-history";
import type { ModelDescriptor, QueueJobStatus, QueueSnapshot } from "../lib/types";

export function queueJobActionState(status: QueueJobStatus) {
  return {
    canRetry: status === "failed" || status === "cancelled",
    canSkip: status === "pending" || status === "failed" || status === "cancelled",
  };
}

export function queueToolbarActionState(
  queue: Pick<QueueSnapshot, "pending_count" | "running_count">,
  canProcessTranscription: boolean,
) {
  return {
    canProcessNext:
      canProcessTranscription && queue.pending_count > 0 && queue.running_count === 0,
    canCancelCurrent: queue.running_count > 0,
  };
}

export function processNextDisabledReason(
  queue: Pick<QueueSnapshot, "pending_count" | "running_count">,
  canProcessTranscription: boolean,
): string | null {
  if (canProcessTranscription && queue.pending_count > 0 && queue.running_count === 0) {
    return null;
  }
  if (queue.running_count > 0) {
    return "A job is already running";
  }
  if (queue.pending_count === 0) {
    return "No pending jobs";
  }
  return "Transcription unavailable";
}

export function cancelCurrentDisabledReason(
  queue: Pick<QueueSnapshot, "running_count">,
): string | null {
  if (queue.running_count > 0) {
    return null;
  }
  return "No running job to cancel";
}

export function queueJobRetryDisabledReason(status: QueueJobStatus): string | null {
  if (queueJobActionState(status).canRetry) {
    return null;
  }
  if (status === "running") {
    return "Job is still running";
  }
  if (status === "completed") {
    return "Job already completed";
  }
  if (status === "skipped") {
    return "Job was skipped";
  }
  return "Job has not run yet";
}

export function queueJobSkipDisabledReason(status: QueueJobStatus): string | null {
  if (queueJobActionState(status).canSkip) {
    return null;
  }
  if (status === "running") {
    return "Job is still running";
  }
  if (status === "completed") {
    return "Job already completed";
  }
  return "Job already skipped";
}

export function QueuePanel({
  queue,
  models,
  canProcessTranscription,
  onImportAudioFiles,
  onEnqueueBacklog,
  onCancelCurrent,
  onProcessNext,
  onRetry,
  onSkip,
}: {
  queue: QueueSnapshot;
  models: ModelDescriptor[];
  canProcessTranscription: boolean;
  onImportAudioFiles: () => void;
  onEnqueueBacklog: () => void;
  onCancelCurrent: () => void;
  onProcessNext: () => void;
  onRetry: (id: number) => void;
  onSkip: (id: number) => void;
}) {
  const toolbarActions = queueToolbarActionState(queue, canProcessTranscription);
  const processNextReason = processNextDisabledReason(queue, canProcessTranscription);
  const cancelCurrentReason = cancelCurrentDisabledReason(queue);
  const groupedJobs = groupQueueJobsByDay(queue.jobs);
  const completedCount = countCompletedQueueJobs(queue.jobs);
  const cancelledCount = countCancelledQueueJobs(queue.jobs);
  const skippedCount = countSkippedQueueJobs(queue.jobs);
  const pendingBreakdown = summarizeQueueJobsByDay(
    queue.jobs.filter((job) => job.status === "pending"),
  );
  const runningBreakdown = summarizeQueueJobsByDay(
    queue.jobs.filter((job) => job.status === "running"),
  );
  const failedBreakdown = summarizeQueueJobsByDay(
    queue.jobs.filter((job) => job.status === "failed"),
  );
  const cancelledBreakdown = summarizeQueueJobsByDay(
    queue.jobs.filter((job) => job.status === "cancelled"),
  );
  const skippedBreakdown = summarizeQueueJobsByDay(
    queue.jobs.filter((job) => job.status === "skipped"),
  );
  const completedBreakdown = summarizeQueueJobsByDay(
    queue.jobs.filter((job) => job.status === "completed"),
  );

  return (
    <div className="queue-panel">
      <div className="queue-stats">
        <div
          data-tone={queue.pending_count > 0 ? queueStatsCellTone("pending") : undefined}
          title={pendingBreakdown || undefined}
        >
          <span>Pending</span>
          <strong>{queue.pending_count}</strong>
        </div>
        <div
          data-tone={skippedCount > 0 ? queueStatsCellTone("skipped") : undefined}
          title={skippedBreakdown || undefined}
        >
          <span>Skipped</span>
          <strong>{skippedCount}</strong>
        </div>
        <div
          data-tone={queue.running_count > 0 ? queueStatsCellTone("running") : undefined}
          title={runningBreakdown || undefined}
        >
          <span>Running</span>
          <strong>{queue.running_count}</strong>
        </div>
        <div
          data-tone={queue.failed_count > 0 ? queueStatsCellTone("failed") : undefined}
          title={failedBreakdown || undefined}
        >
          <span>Failed</span>
          <strong>{queue.failed_count}</strong>
        </div>
        <div
          data-tone={cancelledCount > 0 ? queueStatsCellTone("cancelled") : undefined}
          title={cancelledBreakdown || undefined}
        >
          <span>Cancelled</span>
          <strong>{cancelledCount}</strong>
        </div>
        <div
          data-tone={completedCount > 0 ? queueStatsCellTone("completed") : undefined}
          title={completedBreakdown || undefined}
        >
          <span>Completed</span>
          <strong>{completedCount}</strong>
        </div>
      </div>
      <div className="toolbar">
        <Button type="button" variant="secondary" onClick={onImportAudioFiles}>
          <FileAudio data-icon="inline-start" />
          Import Audio
        </Button>
        <Button type="button" variant="secondary" onClick={onEnqueueBacklog}>
          <FolderInput data-icon="inline-start" />
          Process Backlog
        </Button>
        <Button
          type="button"
          variant="primary"
          onClick={onProcessNext}
          disabled={!toolbarActions.canProcessNext}
          title={processNextReason ?? undefined}
        >
          <Play data-icon="inline-start" />
          Process Next
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={onCancelCurrent}
          disabled={!toolbarActions.canCancelCurrent}
          title={cancelCurrentReason ?? undefined}
        >
          <Ban data-icon="inline-start" />
          Cancel Current
        </Button>
      </div>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Audio</th>
              <th>Model</th>
              <th>Status</th>
              <th>Action</th>
            </tr>
          </thead>
          <tbody>
            {queue.jobs.length === 0 ? (
              <tr>
                <td colSpan={4} className="empty-cell">
                  No queued transcription jobs
                </td>
              </tr>
            ) : (
              groupedJobs.map((group) => {
                const pendingCount = countPendingQueueJobs(group.entries);
                const runningCount = countRunningQueueJobs(group.entries);
                const groupCompletedCount = countCompletedQueueJobs(group.entries);
                const failedCount = countFailedQueueJobs(group.entries);
                const cancelledCount = countCancelledQueueJobs(group.entries);
                const skippedCount = countSkippedQueueJobs(group.entries);
                return (
                <Fragment key={group.day}>
                  <tr className="table-group-row">
                    <td colSpan={4}>
                      {group.day} · {group.entries.length} job{group.entries.length === 1 ? "" : "s"}
                      {pendingCount > 0 ? (
                        <>
                          {" · "}
                          <span data-tone={queueStatsCellTone("pending")}>{pendingCount} pending</span>
                        </>
                      ) : null}
                      {runningCount > 0 ? (
                        <>
                          {" · "}
                          <span data-tone={queueStatsCellTone("running")}>{runningCount} running</span>
                        </>
                      ) : null}
                      {groupCompletedCount > 0 ? (
                        <>
                          {" · "}
                          <span data-tone={queueStatsCellTone("completed")}>{groupCompletedCount} completed</span>
                        </>
                      ) : null}
                      {failedCount > 0 ? (
                        <>
                          {" · "}
                          <span data-tone={queueStatsCellTone("failed")}>{failedCount} failed</span>
                        </>
                      ) : null}
                      {cancelledCount > 0 ? (
                        <>
                          {" · "}
                          <span data-tone={queueStatsCellTone("cancelled")}>{cancelledCount} cancelled</span>
                        </>
                      ) : null}
                      {skippedCount > 0 ? (
                        <>
                          {" · "}
                          <span data-tone={queueStatsCellTone("skipped")}>{skippedCount} skipped</span>
                        </>
                      ) : null}
                    </td>
                  </tr>
                  {group.entries.map((job) => {
                    const actions = queueJobActionState(job.status);
                    const retryReason = queueJobRetryDisabledReason(job.status);
                    const skipReason = queueJobSkipDisabledReason(job.status);
                    const rowTone = queueJobStatusBadgeTone(job.status);
                    const sidecarPath = queueJobSidecarPath(job.audio_path, job.status);
                    const statusBadge = (
                      <Badge tone={queueJobStatusBadgeTone(job.status)}>{humanizeQueueJobStatus(job.status)}</Badge>
                    );
                    return (
                      <tr key={job.id} data-tone={rowTone === "neutral" ? undefined : rowTone}>
                        <td>
                          <a href={fileUrlFromPath(job.audio_path)} title={job.audio_path}>
                            {formatAudioPathLabel(job.audio_path)}
                          </a>
                        </td>
                        <td title={job.model_id}>{formatModelLabel(job.model_id, models)}</td>
                        <td title={job.error ?? undefined}>
                          {sidecarPath ? (
                            <a href={fileUrlFromPath(sidecarPath)} title={sidecarPath}>
                              {statusBadge}
                            </a>
                          ) : (
                            statusBadge
                          )}
                        </td>
                        <td>
                          <div className="row-actions">
                            <Button
                              type="button"
                              variant="secondary"
                              size="icon"
                              title={retryReason ?? "Retry"}
                              onClick={() => onRetry(job.id)}
                              disabled={!actions.canRetry}
                            >
                              <RotateCw />
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              title={skipReason ?? "Skip"}
                              onClick={() => onSkip(job.id)}
                              disabled={!actions.canSkip}
                            >
                              <SkipForward />
                            </Button>
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </Fragment>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
