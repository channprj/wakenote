import {
  AlertCircle,
  CheckCircle2,
  Download,
  FileText,
  Loader2,
  RefreshCw,
  RotateCcw,
} from "lucide-react";
import { useMemo, useState } from "react";
import type { ReportListEntry } from "../lib/llm-report-runs";
import { formatLocalTimestamp } from "../lib/transcript-history";
import type {
  LlmReportHistoryDetail,
  LlmReportKind,
  LlmReportRunSnapshot,
} from "../lib/types";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import { StatusBadge } from "./ui/status-badge";
import { Tabs, TabsList, TabsTrigger } from "./ui/tabs";
import {
  progressStageLabels,
  ReportRunDetail,
  runStatusLabels,
} from "./ReportRunDetail";

type ReportFilter = "all" | LlmReportKind;

export function ReportHistoryView({
  entries,
  selectedKey,
  detail,
  loading,
  detailLoading,
  downloadingId,
  actionPendingRunId,
  error,
  onSelect,
  onRefresh,
  onDownload,
  onCancel,
  onRetry,
}: {
  entries: readonly ReportListEntry[];
  selectedKey: string | null;
  detail: LlmReportHistoryDetail | null;
  loading: boolean;
  detailLoading: boolean;
  downloadingId: string | null;
  actionPendingRunId: string | null;
  error: string | null;
  onSelect: (entryKey: string) => void;
  onRefresh: () => void;
  onDownload: (reportId: string, fileName: string) => void;
  onCancel: (runId: string) => void;
  onRetry: (runId: string) => void;
}) {
  const [filter, setFilter] = useState<ReportFilter>("all");
  const filteredEntries = useMemo(
    () =>
      entries.filter(
        (entry) => filter === "all" || entry.kind === filter,
      ),
    [entries, filter],
  );
  const selectedEntry =
    entries.find((entry) => entry.key === selectedKey) ?? null;
  const filteredDetail =
    detail &&
    selectedEntry?.report?.report_id === detail.item.report_id &&
    (filter === "all" || detail.item.kind === filter)
      ? detail
      : null;

  function changeFilter(value: string) {
    const nextFilter = value as ReportFilter;
    setFilter(nextFilter);
    if (
      nextFilter !== "all" &&
      selectedEntry?.kind !== nextFilter
    ) {
      const firstMatch = entries.find(
        (entry) => entry.kind === nextFilter,
      );
      if (firstMatch) onSelect(firstMatch.key);
    }
  }

  return (
    <div className="report-history">
      <div className="report-history__toolbar">
        <Tabs value={filter} onValueChange={changeFilter}>
          <TabsList
            aria-label="Report type filter"
            className="report-history__filters"
            variant="line"
          >
            {([
              ["all", "All"],
              ["summary", "Summaries"],
              ["detailed_report", "Detailed reports"],
            ] as const).map(([value, label]) => (
              <TabsTrigger key={value} value={value}>
                {label}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <Button
          aria-label="Refresh report history"
          disabled={loading}
          onClick={onRefresh}
          size="sm"
          type="button"
          variant="secondary"
        >
          <RefreshCw
            data-icon="inline-start"
            className={loading ? "loading-spin" : undefined}
          />
          Refresh
        </Button>
      </div>

      {error ? (
        <Alert variant="destructive" className="report-history__error">
          <AlertCircle />
          <AlertTitle>Reports unavailable</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      <div className="report-history__layout">
        <aside aria-label="Generated reports" className="report-history__list">
          {loading && entries.length === 0 ? (
            <div className="report-history__empty">
              <Loader2 className="loading-spin" /> Loading reports
            </div>
          ) : filteredEntries.length === 0 ? (
            <div className="report-history__empty">No reports found</div>
          ) : (
            filteredEntries.map((entry) => (
              <ReportEntryRow
                entry={entry}
                key={entry.key}
                selected={selectedKey === entry.key}
                onSelect={onSelect}
              />
            ))
          )}
        </aside>

        <section
          data-slot="report-detail"
          aria-label="Report detail"
          className="report-history__detail"
        >
          {selectedEntry?.run ? (
            <ReportRunDetail
              actionPending={
                actionPendingRunId === selectedEntry.run.run_id
              }
              run={selectedEntry.run}
              onCancel={onCancel}
              onRetry={onRetry}
            />
          ) : detailLoading ? (
            <div className="report-history__empty">
              <Loader2 className="loading-spin" /> Loading report
            </div>
          ) : filteredDetail && selectedEntry?.report ? (
            <ReportDetail
              actionPending={
                selectedEntry.sourceRun?.run_id === actionPendingRunId
              }
              detail={filteredDetail}
              downloading={
                downloadingId === filteredDetail.item.report_id
              }
              sourceRun={selectedEntry.sourceRun}
              onDownload={onDownload}
              onRetry={onRetry}
            />
          ) : (
            <div className="report-history__empty">Select a report</div>
          )}
        </section>
      </div>
    </div>
  );
}

function ReportEntryRow({
  entry,
  selected,
  onSelect,
}: {
  entry: ReportListEntry;
  selected: boolean;
  onSelect: (entryKey: string) => void;
}) {
  const run = entry.run;
  const report = entry.report;
  return (
    <button
      data-slot="report-row"
      data-kind={run ? "run" : "report"}
      data-status={run?.status}
      aria-current={selected ? "true" : undefined}
      className="report-history__row"
      data-selected={selected}
      onClick={() => onSelect(entry.key)}
      type="button"
    >
      <span className="report-history__row-icon">
        {run && ["queued", "running", "stopping"].includes(run.status) ? (
          <Loader2 className="loading-spin" />
        ) : (
          <FileText />
        )}
      </span>
      <span className="report-history__row-main">
        <strong>{reportKindLabel(entry.kind)}</strong>
        <small>
          {formatLocalTimestamp(entry.createdAt) || entry.createdAt}
        </small>
        <span>{run?.message ?? report?.file_name}</span>
      </span>
      <span className="report-history__row-usage">
        {run ? (
          <>
            <StatusBadge tone={runStatusTone(run)}>
              {runStatusLabels[run.status]}
            </StatusBadge>
            <small>
              {run.stage ? progressStageLabels[run.stage] : "Waiting"}
              {run.iteration > 0
                ? ` · ${run.iteration}/${run.max_iterations}`
                : ""}
            </small>
          </>
        ) : (
          <>
            {report?.usage?.total_tokens == null
              ? "Tokens unavailable"
              : `${formatTokens(report.usage.total_tokens)} tokens`}
            <small>{formatCost(report?.usage?.cost ?? null)}</small>
          </>
        )}
      </span>
    </button>
  );
}

function runStatusTone(
  run: LlmReportRunSnapshot,
): "neutral" | "primary" | "warning" | "danger" | "success" {
  switch (run.status) {
    case "running":
      return "primary";
    case "stopping":
    case "cancelled":
      return "warning";
    case "failed":
      return "danger";
    case "completed":
      return "success";
    default:
      return "neutral";
  }
}

function ReportDetail({
  detail,
  downloading,
  actionPending,
  sourceRun,
  onDownload,
  onRetry,
}: {
  detail: LlmReportHistoryDetail;
  downloading: boolean;
  actionPending: boolean;
  sourceRun: LlmReportRunSnapshot | null;
  onDownload: (reportId: string, fileName: string) => void;
  onRetry: (runId: string) => void;
}) {
  const { item } = detail;
  return (
    <>
      <header className="report-history__detail-header">
        <div>
          <div className="report-history__badges">
            <StatusBadge
              tone={item.kind === "summary" ? "neutral" : "primary"}
            >
              {reportKindLabel(item.kind)}
            </StatusBadge>
            {item.legacy ? (
              <StatusBadge tone="warning">Legacy report</StatusBadge>
            ) : null}
            {item.success_criteria_met === true ? (
              <StatusBadge tone="success">
                <CheckCircle2 data-icon="inline-start" /> Success criteria met
              </StatusBadge>
            ) : item.success_criteria_met === false ? (
              <StatusBadge tone="warning">
                <AlertCircle data-icon="inline-start" /> Maximum iterations
                reached
              </StatusBadge>
            ) : null}
          </div>
          <h3>{item.file_name}</h3>
          <span>
            {formatLocalTimestamp(item.created_at) || item.created_at}
          </span>
        </div>
        <div className="report-history__detail-actions">
          {sourceRun ? (
            <Button
              aria-label="Run report again"
              disabled={actionPending}
              onClick={() => onRetry(sourceRun.run_id)}
              size="sm"
              type="button"
              variant="secondary"
            >
              {actionPending ? (
                <Loader2
                  data-icon="inline-start"
                  className="loading-spin"
                />
              ) : (
                <RotateCcw data-icon="inline-start" />
              )}
              Run again
            </Button>
          ) : null}
          <Button
            disabled={downloading}
            onClick={() => onDownload(item.report_id, item.file_name)}
            size="sm"
            type="button"
            variant="secondary"
          >
            {downloading ? (
              <Loader2
                data-icon="inline-start"
                className="loading-spin"
              />
            ) : (
              <Download data-icon="inline-start" />
            )}
            Download Markdown
          </Button>
        </div>
      </header>

      <dl className="report-history__metadata">
        <Metadata
          label="Model"
          value={item.model ?? "Not recorded"}
          title={item.model ?? undefined}
        />
        <Metadata
          label="Iterations"
          value={
            item.iterations_used == null || item.max_iterations == null
              ? "Not recorded"
              : `${item.iterations_used} of ${item.max_iterations}`
          }
        />
        <Metadata
          label="Transcripts"
          value={
            item.selected_count == null
              ? "Not recorded"
              : formatTokens(item.selected_count)
          }
        />
        <Metadata
          label="Date range"
          value={item.date_range ?? "Not recorded"}
        />
      </dl>

      <div className="report-history__usage" aria-label="LLM usage">
        <UsageValue
          label="Input tokens"
          value={item.usage?.prompt_tokens ?? null}
        />
        <UsageValue
          label="Output tokens"
          value={item.usage?.completion_tokens ?? null}
        />
        <UsageValue
          label="Total tokens"
          value={item.usage?.total_tokens ?? null}
        />
        <div>
          <span>OpenRouter cost</span>
          <strong>{formatCost(item.usage?.cost ?? null)}</strong>
          <small>
            {item.usage
              ? `${item.usage.request_count} API requests`
              : "Not recorded"}
          </small>
        </div>
      </div>

      {item.quality_feedback ? (
        <div className="report-history__feedback">
          <span>Quality feedback</span>
          <p>{item.quality_feedback}</p>
        </div>
      ) : null}

      <pre className="report-history__content">{detail.content}</pre>
    </>
  );
}

function Metadata({
  label,
  value,
  title,
}: {
  label: string;
  value: string;
  title?: string;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd title={title}>{value}</dd>
    </div>
  );
}

function UsageValue({
  label,
  value,
}: {
  label: string;
  value: number | null;
}) {
  return (
    <div>
      <span>{label}</span>
      <strong>
        {value == null ? "Not recorded" : formatTokens(value)}
      </strong>
    </div>
  );
}

function reportKindLabel(kind: LlmReportKind): string {
  return kind === "summary" ? "Summary" : "Detailed report";
}

function formatTokens(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

function formatCost(value: number | null): string {
  if (value == null) return "Not recorded";
  return `${value.toFixed(value < 0.01 ? 6 : 4)} credits`;
}
