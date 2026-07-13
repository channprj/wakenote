import { describe, expect, it } from "vitest";
import type { LlmReportHistoryItem } from "../lib/types";
import { historySelectionAfterRefresh } from "./ReportHistoryPanel";

function report(reportId: string): LlmReportHistoryItem {
  return {
    report_id: reportId,
    kind: "summary",
    created_at: "2026-07-13T10:00:00+09:00",
    file_name: `${reportId}.md`,
    report_path: `/tmp/reports/${reportId}.md`,
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

describe("historySelectionAfterRefresh", () => {
  it("advances the detail reload revision when the selected report is unchanged", () => {
    const current = { selectedId: "report-a-summary", detailReloadRevision: 2 };

    const next = historySelectionAfterRefresh(current, [report("report-a-summary")]);

    expect(next).toEqual({
      selectedId: "report-a-summary",
      detailReloadRevision: 3,
    });
  });
});
