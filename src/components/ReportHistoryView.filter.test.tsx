// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ReportListEntry } from "../lib/llm-report-runs";
import type { LlmReportHistoryItem } from "../lib/types";
import { ReportHistoryView } from "./ReportHistoryView";

function historyEntry(
  overrides: Partial<LlmReportHistoryItem>,
): ReportListEntry {
  const item: LlmReportHistoryItem = {
    report_id: "report-1",
    kind: "summary",
    created_at: "2026-07-27T09:24:12+09:00",
    file_name: "20260727-summary.md",
    report_path: "/tmp/reports/20260727-summary.md",
    model: "z-ai/glm-5.2",
    iterations_used: 1,
    max_iterations: 3,
    success_criteria_met: true,
    completion_reason: "success_criteria_met",
    quality_feedback: null,
    selected_count: 4,
    date_range: "2026-07-27",
    usage: null,
    legacy: false,
    ...overrides,
  };
  return {
    key: `report:${item.report_id}`,
    kind: item.kind,
    createdAt: item.created_at,
    run: null,
    report: item,
    sourceRun: null,
  };
}

const entries: ReportListEntry[] = [
  historyEntry({}),
  historyEntry({
    report_id: "report-2",
    kind: "detailed_report",
    file_name: "20260726-detailed-report.md",
    model: "anthropic/claude-sonnet-5",
    date_range: "2026-07-26",
    created_at: "2026-07-26T15:02:38+09:00",
  }),
];

function renderView(list: readonly ReportListEntry[] = entries) {
  render(
    <ReportHistoryView
      actionPendingRunId={null}
      detail={null}
      detailLoading={false}
      downloadingId={null}
      entries={list}
      error={null}
      loading={false}
      selectedKey={list[0]?.key ?? null}
      onCancel={vi.fn()}
      onDownload={vi.fn()}
      onRefresh={vi.fn()}
      onRetry={vi.fn()}
      onSelect={vi.fn()}
    />,
  );
}

function listedFileNames() {
  return screen
    .getAllByRole("button")
    .map((button) => button.textContent ?? "")
    .filter((text) => text.includes(".md"));
}

afterEach(cleanup);

describe("ReportHistoryView filtering", () => {
  it("lists every report before a filter is typed", () => {
    renderView();

    const names = listedFileNames();
    expect(names.some((name) => name.includes("20260727-summary.md"))).toBe(true);
    expect(
      names.some((name) => name.includes("20260726-detailed-report.md")),
    ).toBe(true);
  });

  it("narrows the list to matching reports as the user types", async () => {
    const user = userEvent.setup();
    renderView();

    await user.type(screen.getByLabelText("Filter reports"), "claude");

    const names = listedFileNames();
    expect(
      names.some((name) => name.includes("20260726-detailed-report.md")),
    ).toBe(true);
    expect(names.some((name) => name.includes("20260727-summary.md"))).toBe(
      false,
    );
  });

  it("explains an empty result and offers to clear the filter", async () => {
    const user = userEvent.setup();
    renderView();

    await user.type(screen.getByLabelText("Filter reports"), "nonexistent");

    expect(screen.getByText("No reports match this filter")).toBeTruthy();
    expect(screen.getByText(/nonexistent/)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Clear filter" }));

    expect(listedFileNames().length).toBeGreaterThan(0);
  });

  it("keeps the no-reports-yet state when the list is genuinely empty", () => {
    renderView([]);

    expect(screen.getByText("No reports yet")).toBeTruthy();
  });

  it("closes the detail pane when the filter hides the selected report", async () => {
    const user = userEvent.setup();
    render(
      <ReportHistoryView
        actionPendingRunId={null}
        detail={{ item: entries[0].report!, content: "# Saved summary" }}
        detailLoading={false}
        downloadingId={null}
        entries={entries}
        error={null}
        loading={false}
        selectedKey={entries[0].key}
        onCancel={vi.fn()}
        onDownload={vi.fn()}
        onRefresh={vi.fn()}
        onRetry={vi.fn()}
        onSelect={vi.fn()}
      />,
    );

    expect(screen.getByRole("heading", { name: "Saved summary" })).toBeTruthy();

    await user.type(screen.getByLabelText("Filter reports"), "claude");

    expect(screen.queryByRole("heading", { name: "Saved summary" })).toBeNull();
    expect(screen.getByText("Select a report")).toBeTruthy();
  });
});
