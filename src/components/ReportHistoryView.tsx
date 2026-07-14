import {
  AlertCircle,
  CheckCircle2,
  Download,
  FileText,
  Loader2,
  RefreshCw,
} from "lucide-react";
import { useMemo, useState } from "react";
import { formatLocalTimestamp } from "../lib/transcript-history";
import type {
  LlmReportHistoryDetail,
  LlmReportHistoryItem,
  LlmReportKind,
} from "../lib/types";
import { Alert, AlertDescription, AlertTitle } from "./ui/alert";
import { Button } from "./ui/button";
import { StatusBadge } from "./ui/status-badge";
import { Tabs, TabsList, TabsTrigger } from "./ui/tabs";

type ReportFilter = "all" | LlmReportKind;

export function ReportHistoryView({
  items,
  selectedId,
  detail,
  loading,
  detailLoading,
  downloadingId,
  error,
  onSelect,
  onRefresh,
  onDownload,
}: {
  items: readonly LlmReportHistoryItem[];
  selectedId: string | null;
  detail: LlmReportHistoryDetail | null;
  loading: boolean;
  detailLoading: boolean;
  downloadingId: string | null;
  error: string | null;
  onSelect: (reportId: string) => void;
  onRefresh: () => void;
  onDownload: (reportId: string, fileName: string) => void;
}) {
  const [filter, setFilter] = useState<ReportFilter>("all");
  const filteredItems = useMemo(
    () => items.filter((item) => filter === "all" || item.kind === filter),
    [filter, items],
  );
  const filteredDetail =
    detail &&
    detail.item.report_id === selectedId &&
    (filter === "all" || detail.item.kind === filter)
      ? detail
      : null;

  function changeFilter(value: string) {
    const nextFilter = value as ReportFilter;
    setFilter(nextFilter);
    if (nextFilter !== "all" && detail?.item.kind !== nextFilter) {
      const firstMatch = items.find((item) => item.kind === nextFilter);
      if (firstMatch) onSelect(firstMatch.report_id);
    }
  }

  return (
    <div className="report-history">
      <div className="report-history__toolbar">
        <Tabs value={filter} onValueChange={changeFilter}>
          <TabsList aria-label="Report type filter" className="report-history__filters" variant="line">
            {([
              ["all", "All"],
              ["summary", "Summaries"],
              ["detailed_report", "Detailed reports"],
            ] as const).map(([value, label]) => (
              <TabsTrigger key={value} value={value}>{label}</TabsTrigger>
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
          <RefreshCw data-icon="inline-start" className={loading ? "transcript-footer__spin" : undefined} />
          Refresh
        </Button>
      </div>

      {error ? (
        <Alert variant="destructive" className="report-history__error">
          <AlertCircle />
          <AlertTitle>Report history unavailable</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}

      <div className="report-history__layout">
        <aside aria-label="Generated reports" className="report-history__list">
          {loading && items.length === 0 ? (
            <div className="report-history__empty">
              <Loader2 className="transcript-footer__spin" /> Loading reports
            </div>
          ) : filteredItems.length === 0 ? (
            <div className="report-history__empty">No reports found</div>
          ) : (
            filteredItems.map((item) => (
              <button
                data-slot="report-row"
                aria-current={selectedId === item.report_id ? "true" : undefined}
                className="report-history__row"
                data-selected={selectedId === item.report_id}
                key={item.report_id}
                onClick={() => onSelect(item.report_id)}
                type="button"
              >
                <span className="report-history__row-icon"><FileText /></span>
                <span className="report-history__row-main">
                  <strong>{reportKindLabel(item.kind)}</strong>
                  <small>{formatLocalTimestamp(item.created_at) || item.created_at}</small>
                  <span>{item.file_name}</span>
                </span>
                <span className="report-history__row-usage">
                  {item.usage?.total_tokens == null
                    ? "Tokens unavailable"
                    : `${formatTokens(item.usage.total_tokens)} tokens`}
                  <small>{formatCost(item.usage?.cost ?? null)}</small>
                </span>
              </button>
            ))
          )}
        </aside>

        <section data-slot="report-detail" aria-label="Report detail" className="report-history__detail">
          {detailLoading ? (
            <div className="report-history__empty">
              <Loader2 className="transcript-footer__spin" /> Loading report
            </div>
          ) : filteredDetail ? (
            <ReportDetail
              detail={filteredDetail}
              downloading={downloadingId === filteredDetail.item.report_id}
              onDownload={onDownload}
            />
          ) : (
            <div className="report-history__empty">Select a report</div>
          )}
        </section>
      </div>
    </div>
  );
}

function ReportDetail({
  detail,
  downloading,
  onDownload,
}: {
  detail: LlmReportHistoryDetail;
  downloading: boolean;
  onDownload: (reportId: string, fileName: string) => void;
}) {
  const { item } = detail;
  return (
    <>
      <header className="report-history__detail-header">
        <div>
          <div className="report-history__badges">
            <StatusBadge tone={item.kind === "summary" ? "neutral" : "primary"}>
              {reportKindLabel(item.kind)}
            </StatusBadge>
            {item.legacy ? <StatusBadge tone="warning">Legacy report</StatusBadge> : null}
            {item.success_criteria_met === true ? (
              <StatusBadge tone="success"><CheckCircle2 data-icon="inline-start" /> Success criteria met</StatusBadge>
            ) : item.success_criteria_met === false ? (
              <StatusBadge tone="warning"><AlertCircle data-icon="inline-start" /> Maximum iterations reached</StatusBadge>
            ) : null}
          </div>
          <h3>{item.file_name}</h3>
          <span>{formatLocalTimestamp(item.created_at) || item.created_at}</span>
        </div>
        <Button
          disabled={downloading}
          onClick={() => onDownload(item.report_id, item.file_name)}
          size="sm"
          type="button"
          variant="secondary"
        >
          {downloading ? <Loader2 data-icon="inline-start" className="transcript-footer__spin" /> : <Download data-icon="inline-start" />}
          Download Markdown
        </Button>
      </header>

      <dl className="report-history__metadata">
        <Metadata label="Model" value={item.model ?? "Not recorded"} title={item.model ?? undefined} />
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
          value={item.selected_count == null ? "Not recorded" : formatTokens(item.selected_count)}
        />
        <Metadata label="Date range" value={item.date_range ?? "Not recorded"} />
      </dl>

      <div className="report-history__usage" aria-label="LLM usage">
        <UsageValue label="Input tokens" value={item.usage?.prompt_tokens ?? null} />
        <UsageValue label="Output tokens" value={item.usage?.completion_tokens ?? null} />
        <UsageValue label="Total tokens" value={item.usage?.total_tokens ?? null} />
        <div>
          <span>OpenRouter cost</span>
          <strong>{formatCost(item.usage?.cost ?? null)}</strong>
          <small>
            {item.usage ? `${item.usage.request_count} API requests` : "Not recorded"}
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

function Metadata({ label, value, title }: { label: string; value: string; title?: string }) {
  return <div><dt>{label}</dt><dd title={title}>{value}</dd></div>;
}

function UsageValue({ label, value }: { label: string; value: number | null }) {
  return <div><span>{label}</span><strong>{value == null ? "Not recorded" : formatTokens(value)}</strong></div>;
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
