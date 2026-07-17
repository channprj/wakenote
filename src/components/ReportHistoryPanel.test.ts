import { describe, expect, it } from "vitest";
import { combineReportEntries } from "../lib/llm-report-runs";
import type {
  LlmReportHistoryItem,
  LlmReportRunSnapshot,
} from "../lib/types";
import { reportSelectionAfterRefresh } from "./ReportHistoryPanel";

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

describe("reportSelectionAfterRefresh", () => {
  it("hands a selected completed run to its linked history entry", () => {
    const item = report("report-a-summary");
    const completed = reportRun({
      run_id: "run-a",
      status: "completed",
      stage: "completed",
      report_id: item.report_id,
    });
    const entries = combineReportEntries([completed], [item]);

    expect(reportSelectionAfterRefresh(
      { selectedKey: "run:run-a", detailReloadRevision: 2 },
      entries,
    )).toEqual({
      selectedKey: "report:report-a-summary",
      detailReloadRevision: 3,
    });
  });
});
