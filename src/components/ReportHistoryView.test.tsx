import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { ReportListEntry } from "../lib/llm-report-runs";
import type {
  LlmReportHistoryDetail,
  LlmReportHistoryItem,
  LlmReportRunSnapshot,
} from "../lib/types";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import { ReportHistoryView } from "./ReportHistoryView";
import { ReportRunDetail } from "./ReportRunDetail";

function historyItem(overrides: Partial<LlmReportHistoryItem> = {}): LlmReportHistoryItem {
  return {
    report_id: "20260713-100000-summary",
    kind: "summary",
    created_at: "2026-07-13T10:00:00+09:00",
    file_name: "20260713-100000-summary.md",
    report_path: "/tmp/reports/20260713-100000-summary.md",
    model: "z-ai/glm-5.2",
    iterations_used: 1,
    max_iterations: 3,
    success_criteria_met: true,
    completion_reason: "success_criteria_met",
    quality_feedback: "Complete and grounded.",
    selected_count: 2,
    date_range: "2026-07-13T09:00:00+09:00 - 2026-07-13T10:00:00+09:00",
    usage: {
      request_count: 2,
      prompt_tokens: 180,
      completion_tokens: 60,
      total_tokens: 240,
      cost: 0.0015,
    },
    legacy: false,
    ...overrides,
  };
}

function reportRun(
  overrides: Partial<LlmReportRunSnapshot> = {},
): LlmReportRunSnapshot {
  const run: LlmReportRunSnapshot = {
    run_id: "run-1",
    parent_run_id: null,
    revision: 3,
    status: "running",
    stage: "generating",
    kind: "summary",
    created_at: "2026-07-18T00:00:00Z",
    updated_at: "2026-07-18T00:00:02Z",
    started_at: "2026-07-18T00:00:01Z",
    finished_at: null,
    iteration: 1,
    max_iterations: 3,
    message: "Drafting report",
    detail: null,
    error: null,
    progress: [],
    model: "z-ai/glm-5.2",
    selected_count: 2,
    date_range: "2026-07-18",
    report_id: null,
    report_path: null,
    completion_reason: null,
    success_criteria_met: null,
    quality_feedback: null,
    usage: null,
    ...overrides,
  };
  return {
    ...run,
    progress: overrides.progress ?? [{
      run_id: run.run_id,
      stage: run.stage ?? "preparing",
      iteration: run.iteration,
      max_iterations: run.max_iterations,
      message: run.message,
      detail: run.detail,
    }],
  };
}

function renderReportEntries(entries: readonly ReportListEntry[]) {
  return renderToStaticMarkup(
    <ReportHistoryView
      actionPendingRunId={null}
      detail={null}
      detailLoading={false}
      downloadingId={null}
      entries={entries}
      error={null}
      loading={false}
      selectedKey={entries[0]?.key ?? null}
      onCancel={vi.fn()}
      onDownload={vi.fn()}
      onRefresh={vi.fn()}
      onRetry={vi.fn()}
      onSelect={vi.fn()}
    />,
  );
}

function renderRunDetail(run: LlmReportRunSnapshot) {
  return renderToStaticMarkup(
    <ReportRunDetail
      actionPending={false}
      run={run}
      onCancel={vi.fn()}
      onRetry={vi.fn()}
    />,
  );
}

function renderHistory(
  item: LlmReportHistoryItem,
  detail: LlmReportHistoryDetail | null = { item, content: "# Saved summary" },
  sourceRun: LlmReportRunSnapshot | null = null,
) {
  const entry: ReportListEntry = {
    key: `report:${item.report_id}`,
    kind: item.kind,
    createdAt: item.created_at,
    run: null,
    report: item,
    sourceRun,
  };
  return renderToStaticMarkup(
    <ReportHistoryView
      actionPendingRunId={null}
      detail={detail}
      detailLoading={false}
      downloadingId={null}
      entries={[entry]}
      error={null}
      loading={false}
      selectedKey={entry.key}
      onCancel={vi.fn()}
      onDownload={vi.fn()}
      onRefresh={vi.fn()}
      onRetry={vi.fn()}
      onSelect={vi.fn()}
    />,
  );
}

describe("ReportHistoryView", () => {
  it("renders compact rows and a long-value-safe detail", () => {
    const item = historyItem({
      file_name: `${LONG_CONTENT.token}.md`,
      model: LONG_CONTENT.model,
    });
    const markup = renderHistory(item, {
      item,
      content: `${LONG_CONTENT.korean}\n${LONG_CONTENT.url}`,
    });

    expect(markup).toContain('data-slot="report-row"');
    expect(markup).toContain(`title="${LONG_CONTENT.model}"`);
    expect(markup).toContain(LONG_CONTENT.token);
    expect(markup).toContain(LONG_CONTENT.korean);
    expect(markup).toContain(LONG_CONTENT.url);
    expect(markup).toContain('data-slot="report-detail"');
  });

  it("uses an Alert instead of the legacy global error banner", () => {
    const item = historyItem();
    const entry: ReportListEntry = {
      key: `report:${item.report_id}`,
      kind: item.kind,
      createdAt: item.created_at,
      run: null,
      report: item,
      sourceRun: null,
    };
    const markup = renderToStaticMarkup(
      <ReportHistoryView
        actionPendingRunId={null}
        detail={null}
        detailLoading={false}
        downloadingId={null}
        entries={[entry]}
        error="OpenRouter history is unavailable"
        loading={false}
        selectedKey={entry.key}
        onCancel={vi.fn()}
        onDownload={vi.fn()}
        onRefresh={vi.fn()}
        onRetry={vi.fn()}
        onSelect={vi.fn()}
      />,
    );

    expect(markup).toContain('role="alert"');
    expect(markup).not.toContain("error-banner");
  });

  it("shows token breakdown, provider cost, completion state, and Markdown download", () => {
    const markup = renderHistory(historyItem());

    expect(markup).toContain("Input tokens");
    expect(markup).toContain(">180<");
    expect(markup).toContain("Output tokens");
    expect(markup).toContain(">60<");
    expect(markup).toContain("Total tokens");
    expect(markup).toContain(">240<");
    expect(markup).toContain("0.001500");
    expect(markup).toContain("Success criteria met");
    expect(markup).toContain("Download Markdown");
    expect(markup).toContain("# Saved summary");
  });

  it("keeps legacy Markdown downloadable while marking usage as unavailable", () => {
    const item = historyItem({
      legacy: true,
      model: null,
      iterations_used: null,
      max_iterations: null,
      success_criteria_met: null,
      completion_reason: null,
      quality_feedback: null,
      selected_count: null,
      date_range: null,
      usage: null,
    });

    const markup = renderHistory(item, { item, content: "# Legacy summary" });

    expect(markup).toContain("Legacy report");
    expect(markup).toContain("Not recorded");
    expect(markup).toContain("Download Markdown");
    expect(markup).toContain("# Legacy summary");
  });

  it("does not display stale detail for a newly selected report", () => {
    const selected = historyItem({ report_id: "20260713-110000-summary" });
    const stale = historyItem({ report_id: "20260713-100000-summary" });
    const entries: ReportListEntry[] = [selected, stale].map((item) => ({
      key: `report:${item.report_id}`,
      kind: item.kind,
      createdAt: item.created_at,
      run: null,
      report: item,
      sourceRun: null,
    }));

    const markup = renderToStaticMarkup(
      <ReportHistoryView
        actionPendingRunId={null}
        detail={{ item: stale, content: "# Stale summary" }}
        detailLoading={false}
        downloadingId={null}
        entries={entries}
        error={null}
        loading={false}
        selectedKey={`report:${selected.report_id}`}
        onCancel={vi.fn()}
        onDownload={vi.fn()}
        onRefresh={vi.fn()}
        onRetry={vi.fn()}
        onSelect={vi.fn()}
      />,
    );

    expect(markup).not.toContain("# Stale summary");
    expect(markup).toContain("Select a report");
  });

  it("shows a live report run with timeline and Stop", () => {
    const run = reportRun({
      status: "running",
      stage: "evaluating",
      message: "Checking success criteria",
      iteration: 1,
      max_iterations: 3,
    });
    const markup = renderReportEntries([{
      key: `run:${run.run_id}`,
      kind: run.kind,
      createdAt: run.created_at,
      run,
      report: null,
      sourceRun: run,
    }]);

    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain("Checking success criteria");
    expect(markup).toContain("Iteration 1 of 3");
    expect(markup).toContain('aria-label="Stop report generation"');
  });

  it("shows Retry for cancelled runs and Run again for linked reports", () => {
    const cancelledMarkup = renderRunDetail(reportRun({
      status: "cancelled",
      stage: "cancelled",
      message: "Report generation cancelled",
    }));
    const item = historyItem();
    const linkedMarkup = renderHistory(item, undefined, reportRun({
      status: "completed",
      stage: "completed",
      report_id: item.report_id,
    }));

    expect(cancelledMarkup).toContain('aria-label="Retry report generation"');
    expect(linkedMarkup).toContain('aria-label="Run report again"');
  });
});
