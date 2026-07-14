import { Ban, FileAudio, FolderInput, Play, RotateCw, SkipForward } from "lucide-react";
import { Fragment } from "react";
import { Button } from "./ui/button";
import { StatusBadge } from "./ui/status-badge";
import { formatModelLabel } from "../lib/models";
import { queueJobStatusBadgeTone, queueStatsCellTone } from "../lib/status-summary";
import {
  fileUrlFromPath,
  formatAudioPathLabel,
  groupQueueJobsByDay,
  humanizeQueueJobStatus,
  queueDayBreakdown,
  queueJobSidecarPath,
  queueStatsBanner,
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
  const statsBanner = queueStatsBanner(queue);

  return (
    <div className="queue-panel">
      <div data-slot="queue-summary" className="queue-stats">
        {statsBanner.map(({ status, label, count, title }) => (
          <div
            key={status}
            data-tone={count > 0 ? queueStatsCellTone(status) : undefined}
            title={title || undefined}
          >
            <span>{label}</span>
            <strong>{count}</strong>
          </div>
        ))}
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
      <div className="table-wrap queue-table-wrap">
        <table data-slot="queue-table">
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
                const breakdown = queueDayBreakdown(group.entries);
                return (
                <Fragment key={group.day}>
                  <tr className="table-group-row">
                    <td colSpan={4}>
                      {group.day} · {group.entries.length} job{group.entries.length === 1 ? "" : "s"}
                      {breakdown.map(({ status, count }) => (
                        <Fragment key={status}>
                          {" · "}
                          <span data-tone={queueStatsCellTone(status)}>{count} {status}</span>
                        </Fragment>
                      ))}
                    </td>
                  </tr>
                  {group.entries.map((job) => {
                    const actions = queueJobActionState(job.status);
                    const retryReason = queueJobRetryDisabledReason(job.status);
                    const skipReason = queueJobSkipDisabledReason(job.status);
                    const rowTone = queueJobStatusBadgeTone(job.status);
                    const sidecarPath = queueJobSidecarPath(job.audio_path, job.status);
                    const statusBadge = (
                      <StatusBadge tone={queueJobStatusBadgeTone(job.status)}>{humanizeQueueJobStatus(job.status)}</StatusBadge>
                    );
                    return (
                      <tr key={job.id} data-tone={rowTone === "neutral" ? undefined : rowTone}>
                        <td className="queue-job__audio">
                          <a className="truncate" href={fileUrlFromPath(job.audio_path)} title={job.audio_path}>
                            {formatAudioPathLabel(job.audio_path)}
                          </a>
                          <small className="queue-job__path" title={job.audio_path}>{job.audio_path}</small>
                        </td>
                        <td className="queue-job__model" title={job.model_id}>{formatModelLabel(job.model_id, models)}</td>
                        <td className="queue-job__status">
                          {sidecarPath ? (
                            <a href={fileUrlFromPath(sidecarPath)} title={sidecarPath}>
                              {statusBadge}
                            </a>
                          ) : (
                            statusBadge
                          )}
                          {job.error ? (
                            <span className="queue-job__error overflow-wrap-anywhere" title={job.error}>
                              {job.error}
                            </span>
                          ) : null}
                        </td>
                        <td className="queue-job__actions-cell">
                          <div className="row-actions">
                            <Button
                              type="button"
                              variant="secondary"
                              size="icon"
                              title={retryReason ?? "Retry"}
                              onClick={() => onRetry(job.id)}
                              disabled={!actions.canRetry}
                            >
                              <RotateCw data-icon="solo" />
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              title={skipReason ?? "Skip"}
                              onClick={() => onSkip(job.id)}
                              disabled={!actions.canSkip}
                            >
                              <SkipForward data-icon="solo" />
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
