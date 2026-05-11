import { Ban, FileAudio, FolderInput, Play, RotateCw, SkipForward } from "lucide-react";
import { Fragment } from "react";
import { Badge, Button } from "./ui/primitives";
import { formatModelLabel } from "../lib/models";
import {
  countFailedQueueJobs,
  countPendingQueueJobs,
  countSkippedQueueJobs,
  formatAudioPathLabel,
  groupQueueJobsByDay,
} from "../lib/transcript-history";
import type { ModelDescriptor, QueueJobStatus, QueueSnapshot } from "../lib/types";

function toneForStatus(status: string) {
  if (status === "running") {
    return "primary";
  }
  if (status === "failed" || status === "cancelled") {
    return "danger";
  }
  if (status === "completed") {
    return "success";
  }
  if (status === "skipped") {
    return "warning";
  }
  return "neutral";
}

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
  const groupedJobs = groupQueueJobsByDay(queue.jobs);

  return (
    <div className="queue-panel">
      <div className="queue-stats">
        <div>
          <span>Pending</span>
          <strong>{queue.pending_count}</strong>
        </div>
        <div>
          <span>Running</span>
          <strong>{queue.running_count}</strong>
        </div>
        <div>
          <span>Failed</span>
          <strong>{queue.failed_count}</strong>
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
        >
          <Play data-icon="inline-start" />
          Process Next
        </Button>
        <Button
          type="button"
          variant="secondary"
          onClick={onCancelCurrent}
          disabled={!toolbarActions.canCancelCurrent}
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
                const failedCount = countFailedQueueJobs(group.entries);
                const skippedCount = countSkippedQueueJobs(group.entries);
                return (
                <Fragment key={group.day}>
                  <tr className="table-group-row">
                    <td colSpan={4}>
                      {group.day} · {group.entries.length} job{group.entries.length === 1 ? "" : "s"}
                      {pendingCount > 0 ? ` · ${pendingCount} pending` : ""}
                      {failedCount > 0 ? ` · ${failedCount} failed` : ""}
                      {skippedCount > 0 ? ` · ${skippedCount} skipped` : ""}
                    </td>
                  </tr>
                  {group.entries.map((job) => {
                    const actions = queueJobActionState(job.status);
                    return (
                      <tr key={job.id}>
                        <td title={job.audio_path}>{formatAudioPathLabel(job.audio_path)}</td>
                        <td title={job.model_id}>{formatModelLabel(job.model_id, models)}</td>
                        <td title={job.error ?? undefined}>
                          <Badge tone={toneForStatus(job.status)}>{job.status}</Badge>
                        </td>
                        <td>
                          <div className="row-actions">
                            <Button
                              type="button"
                              variant="secondary"
                              size="icon"
                              title="Retry"
                              onClick={() => onRetry(job.id)}
                              disabled={!actions.canRetry}
                            >
                              <RotateCw />
                            </Button>
                            <Button
                              type="button"
                              variant="ghost"
                              size="icon"
                              title="Skip"
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
