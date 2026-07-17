import { AlertCircle, Loader2, RotateCcw, Square } from "lucide-react";
import { isActiveLlmReportRun } from "../lib/llm-report-runs";
import type { StatusTone } from "../lib/status-summary";
import type {
  LlmProgressStage,
  LlmReportRunSnapshot,
  LlmReportRunStatus,
} from "../lib/types";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import { StatusBadge } from "./ui/status-badge";

export const runStatusLabels: Record<LlmReportRunStatus, string> = {
  queued: "Queued",
  running: "Running",
  stopping: "Stopping",
  cancelled: "Cancelled",
  failed: "Failed",
  completed: "Completed",
};

const runStatusTones: Record<LlmReportRunStatus, StatusTone> = {
  queued: "neutral",
  running: "primary",
  stopping: "warning",
  cancelled: "warning",
  failed: "danger",
  completed: "success",
};

export const progressStageLabels: Record<LlmProgressStage, string> = {
  preparing: "Preparing",
  generating: "Generating",
  evaluating: "Evaluating",
  refining: "Refining",
  saving: "Saving",
  completed: "Completed",
  max_iterations_reached: "Maximum iterations reached",
  failed: "Failed",
  cancelled: "Cancelled",
};

export function ReportRunDetail({
  run,
  actionPending,
  onCancel,
  onRetry,
}: {
  run: LlmReportRunSnapshot;
  actionPending: boolean;
  onCancel: (runId: string) => void;
  onRetry: (runId: string) => void;
}) {
  const active = isActiveLlmReportRun(run);
  return (
    <section
      aria-label="Report run detail"
      aria-live="polite"
      className="report-run-detail"
      data-status={run.status}
    >
      <header className="report-history__detail-header">
        <div>
          <div className="report-history__badges">
            <StatusBadge tone={runStatusTones[run.status]}>
              {runStatusLabels[run.status]}
            </StatusBadge>
            {run.stage ? (
              <StatusBadge tone="neutral">
                {progressStageLabels[run.stage]}
              </StatusBadge>
            ) : null}
          </div>
          <h3>{run.kind === "summary" ? "Summary" : "Detailed report"}</h3>
          <span>{run.message}</span>
        </div>
        {active ? (
          <Button
            aria-label="Stop report generation"
            disabled={actionPending || run.status === "stopping"}
            onClick={() => onCancel(run.run_id)}
            size="sm"
            type="button"
            variant="destructive"
          >
            {actionPending || run.status === "stopping" ? (
              <Loader2 data-icon="inline-start" className="loading-spin" />
            ) : (
              <Square data-icon="inline-start" />
            )}
            {run.status === "stopping" ? "Stopping" : "Stop"}
          </Button>
        ) : ["cancelled", "failed"].includes(run.status) ? (
          <Button
            aria-label="Retry report generation"
            disabled={actionPending}
            onClick={() => onRetry(run.run_id)}
            size="sm"
            type="button"
            variant="secondary"
          >
            {actionPending ? (
              <Loader2 data-icon="inline-start" className="loading-spin" />
            ) : (
              <RotateCcw data-icon="inline-start" />
            )}
            Retry
          </Button>
        ) : null}
      </header>

      <dl className="report-history__metadata">
        <RunMetadata label="Model" value={run.model} />
        <RunMetadata
          label="Iterations"
          value={`${run.iteration} of ${run.max_iterations}`}
        />
        <RunMetadata
          label="Transcripts"
          value={new Intl.NumberFormat("en-US").format(run.selected_count)}
        />
        <RunMetadata
          label="Date range"
          value={run.date_range || "Not recorded"}
        />
      </dl>

      <ol
        aria-label="Report generation timeline"
        className="report-run-detail__timeline"
      >
        {run.progress.length === 0 ? (
          <li
            className="report-run-detail__timeline-item"
            data-stage="queued"
          >
            <StatusBadge tone="neutral">Queued</StatusBadge>
            <strong>{run.message}</strong>
            <span>Waiting to start</span>
          </li>
        ) : run.progress.map((event, index) => (
          <li
            className="report-run-detail__timeline-item"
            data-stage={event.stage}
            key={`${event.stage}-${index}`}
          >
            <StatusBadge
              tone={
                event.stage === "failed"
                  ? "danger"
                  : event.stage === "cancelled"
                    ? "warning"
                    : event.stage === "completed"
                      ? "success"
                      : "neutral"
              }
            >
              {progressStageLabels[event.stage]}
            </StatusBadge>
            <strong>{event.message}</strong>
            <span>
              {event.iteration > 0
                ? `Iteration ${event.iteration} of ${event.max_iterations}`
                : "Before the first iteration"}
            </span>
            {event.detail ? <p>{event.detail}</p> : null}
          </li>
        ))}
      </ol>

      {run.error ? (
        <Alert variant="destructive" className="report-run-detail__alert">
          <AlertCircle />
          <AlertTitle>Report generation failed</AlertTitle>
          <AlertDescription>{run.error}</AlertDescription>
        </Alert>
      ) : run.quality_feedback ? (
        <div className="report-history__feedback">
          <span>Quality feedback</span>
          <p>{run.quality_feedback}</p>
        </div>
      ) : null}
    </section>
  );
}

function RunMetadata({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}
