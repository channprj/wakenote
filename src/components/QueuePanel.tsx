import {
  Ban,
  CheckCheck,
  ChevronLeft,
  ChevronRight,
  FileAudio,
  FolderInput,
  ListChecksIcon,
  Play,
  RotateCw,
  SkipForward,
} from "lucide-react";
import { Fragment, useState } from "react";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from "./ui/alert-dialog";
import { Button } from "./ui/button";
import { EmptyState } from "./ui/empty-state";
import { StatusBadge } from "./ui/status-badge";
import { formatModelLabel } from "../lib/models";
import { queueJobStatusBadgeTone, queueStatsCellTone } from "../lib/status-summary";
import {
  fileUrlFromPath,
  countUnreadActivityOutcomes,
  formatAudioPathLabel,
  groupQueueJobsByDay,
  humanizeQueueJobStatus,
  queueDayBreakdown,
  queueJobSidecarPath,
  queueStatsBanner,
} from "../lib/transcript-history";
import type {
  ModelDescriptor,
  QueueJob,
  QueueJobStatus,
  QueueSnapshot,
} from "../lib/types";

export const ACTIVITY_PAGE_SIZE = 50;
export type ActivityView = "all" | "attention" | "resolved";

function isAttentionOutcome(job: QueueJob): boolean {
  return ["failed", "cancelled", "skipped"].includes(job.status);
}

export function filterActivityJobs(
  jobs: QueueJob[],
  view: ActivityView,
): QueueJob[] {
  if (view === "attention") {
    return jobs.filter((job) => isAttentionOutcome(job) && job.is_read !== true);
  }
  if (view === "resolved") {
    return jobs.filter((job) => isAttentionOutcome(job) && job.is_read === true);
  }
  return jobs;
}

export function activityPage(
  jobs: QueueJob[],
  requestedPage: number,
  pageSize = ACTIVITY_PAGE_SIZE,
) {
  const orderedJobs = groupQueueJobsByDay(jobs).flatMap((group) => group.entries);
  const total = orderedJobs.length;
  const pageCount = Math.max(1, Math.ceil(total / pageSize));
  const page = Math.min(Math.max(1, Math.floor(requestedPage)), pageCount);
  const offset = (page - 1) * pageSize;
  const pageJobs = orderedJobs.slice(offset, offset + pageSize);

  return {
    jobs: pageJobs,
    page,
    pageCount,
    rangeStart: total === 0 ? 0 : offset + 1,
    rangeEnd: offset + pageJobs.length,
    total,
  };
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
  onMarkAllRead,
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
  onMarkAllRead: () => void;
  onCancelCurrent: () => void;
  onProcessNext: () => void;
  onRetry: (id: number) => void;
  onSkip: (id: number) => void;
}) {
  const [requestedPage, setRequestedPage] = useState(1);
  const [activityView, setActivityView] = useState<ActivityView>("all");
  const toolbarActions = queueToolbarActionState(queue, canProcessTranscription);
  const processNextReason = processNextDisabledReason(queue, canProcessTranscription);
  const cancelCurrentReason = cancelCurrentDisabledReason(queue);
  const filteredJobs = filterActivityJobs(queue.jobs, activityView);
  const pagination = activityPage(filteredJobs, requestedPage);
  const groupedJobs = groupQueueJobsByDay(pagination.jobs);
  const statsBanner = queueStatsBanner(queue);
  const unreadOutcomeCount = countUnreadActivityOutcomes(queue.jobs);
  const resolvedOutcomeCount = filterActivityJobs(queue.jobs, "resolved").length;
  const activityViews: Array<{ id: ActivityView; label: string; count: number }> = [
    { id: "all", label: "All", count: queue.jobs.length },
    { id: "attention", label: "Needs attention", count: unreadOutcomeCount },
    { id: "resolved", label: "Resolved", count: resolvedOutcomeCount },
  ];
  const emptyCopy =
    activityView === "attention"
      ? {
          title: "No jobs need attention",
          description: "New failed, cancelled, or skipped outcomes will appear here.",
        }
      : activityView === "resolved"
        ? {
            title: "No resolved issues",
            description: "Outcomes marked as resolved remain available here for review.",
          }
        : {
            title: "No queued transcription jobs",
            description:
              "Captures queue here automatically when transcription is on. You can also import audio files or scan the save folder for a backlog.",
          };

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
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button type="button" variant="secondary">
              <FolderInput data-icon="inline-start" />
              Process Backlog
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>Process the entire backlog?</AlertDialogTitle>
              <AlertDialogDescription>
                This will scan the full save folder and queue every supported audio file
                without a transcript. A large backlog may add many jobs at once.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction onClick={onEnqueueBacklog}>
                Process entire backlog
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
        <Button
          type="button"
          variant="secondary"
          onClick={onMarkAllRead}
          disabled={unreadOutcomeCount === 0}
          title={
            unreadOutcomeCount === 0
              ? "No unresolved outcomes"
              : `Mark ${unreadOutcomeCount} outcomes as resolved`
          }
        >
          <CheckCheck data-icon="inline-start" />
          Mark all resolved
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
      <div className="queue-view-bar">
        <div className="queue-view-tabs" role="group" aria-label="Activity history views">
          {activityViews.map((view) => (
            <Button
              key={view.id}
              type="button"
              size="sm"
              variant={activityView === view.id ? "secondary" : "ghost"}
              aria-pressed={activityView === view.id}
              onClick={() => {
                setActivityView(view.id);
                setRequestedPage(1);
              }}
            >
              {view.label} <span>{view.count}</span>
            </Button>
          ))}
        </div>
        <span className="queue-view-bar__hint">
          Resolved outcomes stay in history until you reprocess them.
        </span>
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
            {filteredJobs.length === 0 ? (
              <tr>
                <td colSpan={4} className="empty-cell">
                  <EmptyState
                    icon={ListChecksIcon}
                    title={emptyCopy.title}
                    description={emptyCopy.description}
                  />
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
                      <tr
                        key={job.id}
                        data-tone={rowTone === "neutral" ? undefined : rowTone}
                        data-read={job.is_read === true ? true : undefined}
                      >
                        <td className="queue-job__audio" data-label="Audio">
                          <a className="truncate" href={fileUrlFromPath(job.audio_path)} title={job.audio_path}>
                            {formatAudioPathLabel(job.audio_path)}
                          </a>
                          <small className="queue-job__path" title={job.audio_path}>{job.audio_path}</small>
                        </td>
                        <td
                          className="queue-job__model"
                          data-label="Model"
                          title={job.model_id}
                        >
                          {formatModelLabel(job.model_id, models)}
                        </td>
                        <td className="queue-job__status" data-label="Status">
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
                          {job.is_read === true && isAttentionOutcome(job) ? (
                            <span className="queue-job__resolution">
                              <CheckCheck aria-hidden="true" /> Resolved
                            </span>
                          ) : null}
                        </td>
                        <td className="queue-job__actions-cell" data-label="Actions">
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
      {pagination.pageCount > 1 ? (
        <nav className="queue-pagination" aria-label="Activity pages">
          <span className="queue-pagination__range">
            {pagination.rangeStart}–{pagination.rangeEnd} of {pagination.total}
          </span>
          <div className="queue-pagination__controls">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="Previous Activity page"
              disabled={pagination.page === 1}
              onClick={() => setRequestedPage(pagination.page - 1)}
            >
              <ChevronLeft data-icon="solo" />
            </Button>
            <span aria-live="polite">
              Page {pagination.page} of {pagination.pageCount}
            </span>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="Next Activity page"
              disabled={pagination.page === pagination.pageCount}
              onClick={() => setRequestedPage(pagination.page + 1)}
            >
              <ChevronRight data-icon="solo" />
            </Button>
          </div>
        </nav>
      ) : null}
    </div>
  );
}
