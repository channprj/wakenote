import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { LlmReportHistoryDetail, LlmReportHistoryItem } from "../lib/types";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import { ReportHistoryView } from "./ReportHistoryView";

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

function renderHistory(
  item: LlmReportHistoryItem,
  detail: LlmReportHistoryDetail | null = { item, content: "# Saved summary" },
) {
  return renderToStaticMarkup(
    <ReportHistoryView
      detail={detail}
      detailLoading={false}
      downloadingId={null}
      error={null}
      items={[item]}
      loading={false}
      selectedId={item.report_id}
      onDownload={vi.fn()}
      onRefresh={vi.fn()}
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
    const markup = renderToStaticMarkup(
      <ReportHistoryView
        detail={null}
        detailLoading={false}
        downloadingId={null}
        error="OpenRouter history is unavailable"
        items={[item]}
        loading={false}
        selectedId={item.report_id}
        onDownload={vi.fn()}
        onRefresh={vi.fn()}
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

    const markup = renderToStaticMarkup(
      <ReportHistoryView
        detail={{ item: stale, content: "# Stale summary" }}
        detailLoading={false}
        downloadingId={null}
        error={null}
        items={[selected, stale]}
        loading={false}
        selectedId={selected.report_id}
        onDownload={vi.fn()}
        onRefresh={vi.fn()}
        onSelect={vi.fn()}
      />,
    );

    expect(markup).not.toContain("# Stale summary");
    expect(markup).toContain("Select a report");
  });
});
