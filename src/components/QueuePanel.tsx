import { Ban, FileAudio, FolderInput, Play, RotateCw, SkipForward } from "lucide-react";
import { Badge, Button } from "./ui/primitives";
import type { QueueJobStatus, QueueSnapshot } from "../lib/types";

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

export function QueuePanel({
  queue,
  onImportAudioFiles,
  onEnqueueBacklog,
  onCancelCurrent,
  onProcessNext,
  onRetry,
  onSkip,
}: {
  queue: QueueSnapshot;
  onImportAudioFiles: () => void;
  onEnqueueBacklog: () => void;
  onCancelCurrent: () => void;
  onProcessNext: () => void;
  onRetry: (id: number) => void;
  onSkip: (id: number) => void;
}) {
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
          disabled={queue.pending_count === 0}
        >
          <Play data-icon="inline-start" />
          Process Next
        </Button>
        <Button type="button" variant="secondary" onClick={onCancelCurrent}>
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
              queue.jobs.map((job) => {
                const actions = queueJobActionState(job.status);
                return (
                  <tr key={job.id}>
                    <td title={job.audio_path}>{job.audio_path}</td>
                    <td>{job.model_id}</td>
                    <td>
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
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
