import {
  AlertCircle,
  Check,
  CheckCircle2,
  Copy,
  Download,
  Eye,
  EyeOff,
  FileText,
  Loader2,
  RefreshCw,
  RotateCcw,
  SearchIcon,
  SparklesIcon,
} from "lucide-react";
import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import {
  filterReportEntries,
  type ReportListEntry,
} from "../lib/llm-report-runs";
import { formatLocalTimestamp } from "../lib/transcript-history";
import type {
  LlmReportHistoryDetail,
  LlmReportKind,
  LlmReportRunSnapshot,
} from "../lib/types";
import {
  ListVisibilityToolbar,
  type ListVisibilityMode,
} from "./ListVisibilityToolbar";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import { Checkbox } from "./ui/checkbox";
import { EmptyState } from "./ui/empty-state";
import { Input } from "./ui/input";
import { MarkdownDocument } from "./ui/markdown";
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
  visibilityMode = "visible",
  visibleCount,
  hiddenCount = 0,
  visibilityMutating = false,
  visibilityStatus = "",
  onVisibilityModeChange,
  onSetEntriesHidden,
  onNewReport,
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
  visibilityMode?: ListVisibilityMode;
  visibleCount?: number;
  hiddenCount?: number;
  visibilityMutating?: boolean;
  visibilityStatus?: string;
  onVisibilityModeChange?: (mode: ListVisibilityMode) => void;
  onSetEntriesHidden?: (
    entries: readonly ReportListEntry[],
    hidden: boolean,
  ) => boolean | Promise<boolean>;
  /** Opens the report composer. Absent when generation is unavailable. */
  onNewReport?: () => void;
}) {
  const [filter, setFilter] = useState<ReportFilter>("all");
  const [query, setQuery] = useState("");
  const [selectedVisibilityKeys, setSelectedVisibilityKeys] =
    useState<Set<string>>(new Set());
  const kindEntries = useMemo(
    () =>
      entries.filter(
        (entry) => filter === "all" || entry.kind === filter,
      ),
    [entries, filter],
  );
  const filteredEntries = useMemo(
    () => filterReportEntries(kindEntries, query),
    [kindEntries, query],
  );
  // Scoped to the filtered list so a row hidden by the kind filter or the search
  // query cannot leave its report open in the detail pane.
  const selectedEntry =
    filteredEntries.find((entry) => entry.key === selectedKey) ?? null;
  const filteredDetail =
    detail && selectedEntry?.report?.report_id === detail.item.report_id
      ? detail
      : null;
  const selectedVisibilityEntries = useMemo(
    () =>
      filteredEntries.filter((entry) =>
        selectedVisibilityKeys.has(entry.key),
      ),
    [filteredEntries, selectedVisibilityKeys],
  );

  useEffect(() => {
    const availableKeys = new Set<string>(
      filteredEntries.map((entry) => entry.key),
    );
    setSelectedVisibilityKeys((current) => {
      const next = new Set(
        [...current].filter((key) => availableKeys.has(key)),
      );
      return next.size === current.size ? current : next;
    });
  }, [filteredEntries, visibilityMode]);

  const clearVisibilitySelection = useCallback(() => {
    setSelectedVisibilityKeys(new Set());
  }, []);

  const applyEntryVisibility = useCallback(
    async (targets: readonly ReportListEntry[]) => {
      if (!onSetEntriesHidden || targets.length === 0) {
        return false;
      }
      const succeeded = await onSetEntriesHidden(
        targets,
        visibilityMode === "visible",
      );
      if (succeeded) {
        clearVisibilitySelection();
      }
      return succeeded;
    },
    [
      clearVisibilitySelection,
      onSetEntriesHidden,
      visibilityMode,
    ],
  );

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
        <div className="report-history__search">
          <SearchIcon aria-hidden="true" />
          <Input
            aria-label="Filter reports"
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter by name, model, or date"
            type="search"
            value={query}
          />
        </div>

        <div className="report-history__toolbar-actions">
          <Button
            aria-label="Refresh report history"
            disabled={loading}
            onClick={onRefresh}
            size="sm"
            type="button"
            variant="ghost"
          >
            <RefreshCw
              data-icon="inline-start"
              className={loading ? "loading-spin" : undefined}
            />
            Refresh
          </Button>
          {onNewReport ? (
            <Button onClick={onNewReport} size="sm" type="button">
              <SparklesIcon data-icon="inline-start" />
              New report
            </Button>
          ) : null}
        </div>
      </div>

      <ListVisibilityToolbar
        mode={visibilityMode}
        visibleCount={
          visibleCount ??
          (visibilityMode === "visible" ? entries.length : 0)
        }
        hiddenCount={
          visibilityMode === "hidden"
            ? Math.max(hiddenCount, entries.length)
            : hiddenCount
        }
        selectedCount={selectedVisibilityEntries.length}
        totalInMode={filteredEntries.length}
        mutating={visibilityMutating}
        statusMessage={visibilityStatus}
        onModeChange={(mode) => {
          clearVisibilitySelection();
          onVisibilityModeChange?.(mode);
        }}
        onSelectAll={() =>
          setSelectedVisibilityKeys(
            new Set(filteredEntries.map((entry) => entry.key)),
          )
        }
        onClearSelection={clearVisibilitySelection}
        onApplySelection={() =>
          void applyEntryVisibility(selectedVisibilityEntries)
        }
      />

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
            <EmptyState
              icon={Loader2}
              iconClassName="loading-spin"
              title="Loading reports"
            />
          ) : filteredEntries.length === 0 ? (
            query.trim() && kindEntries.length > 0 ? (
              <EmptyState
                icon={SearchIcon}
                title="No reports match this filter"
                description={`Nothing here matches “${query.trim()}”.`}
                action={
                  <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    onClick={() => setQuery("")}
                  >
                    Clear filter
                  </Button>
                }
              />
            ) : visibilityMode === "hidden" ? (
              <EmptyState
                icon={EyeOff}
                title="No hidden reports · Files remain on disk"
                description="Hiding a report only removes it from this list. Nothing is deleted from disk."
              />
            ) : (
              <EmptyState
                icon={FileText}
                title={
                  filter === "all"
                    ? "No reports yet"
                    : `No ${reportKindLabel(filter).toLowerCase()} yet`
                }
                description="Reports are written from your captured transcripts. Pick the captures you want covered, then generate a summary or a detailed report."
                action={
                  onNewReport ? (
                    <Button type="button" size="sm" onClick={onNewReport}>
                      <SparklesIcon data-icon="inline-start" />
                      New report
                    </Button>
                  ) : undefined
                }
              />
            )
          ) : (
            filteredEntries.map((entry) => (
              <ReportEntryRow
                entry={entry}
                key={entry.key}
                selected={selectedKey === entry.key}
                visibilitySelected={selectedVisibilityKeys.has(
                  entry.key,
                )}
                visibilityMode={visibilityMode}
                visibilityMutating={visibilityMutating}
                onSelect={onSelect}
                onVisibilitySelectionChange={(key, selected) =>
                  setSelectedVisibilityKeys((current) => {
                    const next = new Set(current);
                    if (selected) {
                      next.add(key);
                    } else {
                      next.delete(key);
                    }
                    return next;
                  })
                }
                onSetHidden={(target) =>
                  void applyEntryVisibility([target])
                }
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
            <EmptyState
              icon={Loader2}
              iconClassName="loading-spin"
              title="Loading report"
            />
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
            <EmptyState
              icon={FileText}
              title="Select a report"
              description={
                filteredEntries.length > 0
                  ? "Choose a report from the list to read it here."
                  : "Generated reports open here as a readable document."
              }
            />
          )}
        </section>
      </div>
    </div>
  );
}

function ReportEntryRow({
  entry,
  selected,
  visibilitySelected,
  visibilityMode,
  visibilityMutating,
  onSelect,
  onVisibilitySelectionChange,
  onSetHidden,
}: {
  entry: ReportListEntry;
  selected: boolean;
  visibilitySelected: boolean;
  visibilityMode: ListVisibilityMode;
  visibilityMutating: boolean;
  onSelect: (entryKey: string) => void;
  onVisibilitySelectionChange: (
    entryKey: string,
    selected: boolean,
  ) => void;
  onSetHidden: (entry: ReportListEntry) => void;
}) {
  const run = entry.run;
  const report = entry.report;
  const kindLabel = reportKindLabel(entry.kind);
  const rowLabel =
    entry.kind === "summary" ? "Summary report" : "Detailed report";
  return (
    <div
      data-slot="report-row"
      data-kind={run ? "run" : "report"}
      data-status={run?.status}
      aria-current={selected ? "true" : undefined}
      className="report-history__row"
      data-selected={selected}
    >
      <Checkbox
        checked={visibilitySelected}
        disabled={visibilityMutating}
        aria-label={`Select ${rowLabel}`}
        onCheckedChange={(checked) =>
          onVisibilitySelectionChange(
            entry.key,
            checked === true,
          )
        }
      />
      <button
        type="button"
        className="report-history__row-select"
        onClick={() => onSelect(entry.key)}
      >
        <span className="report-history__row-icon">
          {run &&
          ["queued", "running", "stopping"].includes(run.status) ? (
            <Loader2 className="loading-spin" />
          ) : (
            <FileText />
          )}
        </span>
        <span className="report-history__row-main">
          <strong>{kindLabel}</strong>
          <small>
            {formatLocalTimestamp(entry.createdAt) ||
              entry.createdAt}
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
                {run.stage
                  ? progressStageLabels[run.stage]
                  : "Waiting"}
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
      <Button
        type="button"
        size="icon-sm"
        variant="ghost"
        disabled={visibilityMutating}
        aria-label={`${visibilityMode === "visible" ? "Hide" : "Restore"} ${rowLabel}`}
        onClick={() => onSetHidden(entry)}
      >
        {visibilityMode === "visible" ? (
          <EyeOff aria-hidden="true" />
        ) : (
          <Eye aria-hidden="true" />
        )}
      </Button>
    </div>
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
  const [copied, setCopied] = useState(false);

  // Reports get pasted into chat and email far more often than they get saved,
  // so copying the Markdown is a first-class action next to Download.
  const copy = useCallback(() => {
    void (async () => {
      try {
        await navigator.clipboard.writeText(detail.content);
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      } catch {
        // Clipboard unavailable — the button simply does not confirm.
      }
    })();
  }, [detail.content]);

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
          <Button
            aria-label="Copy report Markdown"
            onClick={copy}
            size="sm"
            type="button"
            variant="secondary"
          >
            {copied ? (
              <Check data-icon="inline-start" />
            ) : (
              <Copy data-icon="inline-start" />
            )}
            {copied ? "Copied" : "Copy"}
          </Button>
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

      <MarkdownDocument
        className="report-history__content"
        content={detail.content}
      />
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
