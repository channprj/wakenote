// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ReportListEntry } from "@/lib/llm-report-runs";
import type {
  LlmReportHistoryItem,
  LlmReportRunSnapshot,
} from "@/lib/types";
import { ReportHistoryView } from "./ReportHistoryView";

afterEach(cleanup);

function run(): LlmReportRunSnapshot {
  return {
    run_id: "run-1",
    parent_run_id: null,
    revision: 1,
    status: "running",
    stage: "generating",
    kind: "summary",
    created_at: "2026-07-18T00:00:00Z",
    updated_at: "2026-07-18T00:00:01Z",
    started_at: "2026-07-18T00:00:01Z",
    finished_at: null,
    iteration: 1,
    max_iterations: 3,
    message: "Drafting report",
    detail: null,
    error: null,
    progress: [],
    model: "z-ai/glm-5.2",
    selected_count: 1,
    date_range: "2026-07-18",
    report_id: null,
    report_path: null,
    completion_reason: null,
    success_criteria_met: null,
    quality_feedback: null,
    usage: null,
  };
}

function legacyReport(): LlmReportHistoryItem {
  return {
    report_id: "legacy-1",
    kind: "detailed_report",
    created_at: "2026-07-17T00:00:00Z",
    file_name: "legacy-1.md",
    report_path: "/tmp/legacy-1.md",
    model: null,
    iterations_used: null,
    max_iterations: null,
    success_criteria_met: null,
    completion_reason: null,
    quality_feedback: null,
    selected_count: null,
    date_range: null,
    usage: null,
    legacy: true,
  };
}

function entries(): ReportListEntry[] {
  const activeRun = run();
  const report = legacyReport();
  return [
    {
      key: `run:${activeRun.run_id}`,
      kind: activeRun.kind,
      createdAt: activeRun.created_at,
      run: activeRun,
      report: null,
      sourceRun: activeRun,
    },
    {
      key: `report:${report.report_id}`,
      kind: report.kind,
      createdAt: report.created_at,
      run: null,
      report,
      sourceRun: null,
    },
  ];
}

function renderVisibilityView(
  onSetEntriesHidden: (
    entries: readonly ReportListEntry[],
    hidden: boolean,
  ) => boolean | Promise<boolean>,
) {
  const reportEntries = entries();
  render(
    <ReportHistoryView
      actionPendingRunId={null}
      detail={null}
      detailLoading={false}
      downloadingId={null}
      entries={reportEntries}
      error={null}
      loading={false}
      selectedKey={reportEntries[0].key}
      visibleCount={2}
      hiddenCount={0}
      onCancel={vi.fn()}
      onDownload={vi.fn()}
      onRefresh={vi.fn()}
      onRetry={vi.fn()}
      onSelect={vi.fn()}
      onSetEntriesHidden={onSetEntriesHidden}
    />,
  );
  return reportEntries;
}

describe("ReportHistoryView list visibility", () => {
  it("submits active and legacy rows in one hide batch", async () => {
    const onSetEntriesHidden = vi.fn().mockResolvedValue(true);
    const reportEntries = renderVisibilityView(
      onSetEntriesHidden,
    );

    await userEvent.click(
      screen.getByRole("checkbox", {
        name: "Select Summary report",
      }),
    );
    await userEvent.click(
      screen.getByRole("checkbox", {
        name: "Select Detailed report",
      }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Hide selected" }),
    );

    expect(onSetEntriesHidden).toHaveBeenCalledWith(
      reportEntries,
      true,
    );
  });

  it("keeps active report generation independent from Hide", async () => {
    const onCancel = vi.fn();
    const onSetEntriesHidden = vi.fn().mockResolvedValue(true);
    const reportEntries = entries();
    render(
      <ReportHistoryView
        actionPendingRunId={null}
        detail={null}
        detailLoading={false}
        downloadingId={null}
        entries={reportEntries}
        error={null}
        loading={false}
        selectedKey={reportEntries[0].key}
        visibleCount={2}
        hiddenCount={0}
        onCancel={onCancel}
        onDownload={vi.fn()}
        onRefresh={vi.fn()}
        onRetry={vi.fn()}
        onSelect={vi.fn()}
        onSetEntriesHidden={onSetEntriesHidden}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: "Hide Summary report",
      }),
    );

    expect(onSetEntriesHidden).toHaveBeenCalledWith(
      [reportEntries[0]],
      true,
    );
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("preserves selection when the atomic mutation fails", async () => {
    const onSetEntriesHidden = vi.fn().mockResolvedValue(false);
    renderVisibilityView(onSetEntriesHidden);
    const checkbox = screen.getByRole("checkbox", {
      name: "Select Summary report",
    });

    await userEvent.click(checkbox);
    await userEvent.click(
      screen.getByRole("button", { name: "Hide selected" }),
    );

    expect(checkbox.getAttribute("aria-checked")).toBe("true");
  });
});
