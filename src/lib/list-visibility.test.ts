import { describe, expect, it } from "vitest";
import type { ReportListEntry } from "./llm-report-runs";
import {
  applyListVisibilityRequest,
  emptyListVisibilityState,
  projectListItems,
  reportListVisibilityTarget,
} from "./list-visibility";
import type {
  ListVisibilityTarget,
  LlmReportHistoryItem,
  LlmReportRunSnapshot,
} from "./types";

function reportRun(
  overrides: Partial<LlmReportRunSnapshot> = {},
): LlmReportRunSnapshot {
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
}

function historyItem(
  overrides: Partial<LlmReportHistoryItem> = {},
): LlmReportHistoryItem {
  return {
    report_id: "report-1",
    kind: "summary",
    created_at: "2026-07-18T00:01:00Z",
    file_name: "report-1.md",
    report_path: "/tmp/report-1.md",
    model: "z-ai/glm-5.2",
    iterations_used: 1,
    max_iterations: 3,
    success_criteria_met: true,
    completion_reason: "success_criteria_met",
    quality_feedback: "Complete",
    selected_count: 2,
    date_range: "2026-07-18",
    usage: null,
    legacy: false,
    ...overrides,
  };
}

function runEntry(run = reportRun()): ReportListEntry {
  return {
    key: `run:${run.run_id}`,
    kind: run.kind,
    createdAt: run.created_at,
    run,
    report: null,
    sourceRun: run,
  };
}

function completedReportEntry(
  sourceRun: LlmReportRunSnapshot | null,
): ReportListEntry {
  const report = historyItem();
  return {
    key: `report:${report.report_id}`,
    kind: report.kind,
    createdAt: report.created_at,
    run: null,
    report,
    sourceRun,
  };
}

describe("list visibility", () => {
  it("applies and restores a mixed-kind request without mutating input", () => {
    const initial = emptyListVisibilityState();
    const targets: ListVisibilityTarget[] = [
      { kind: "meeting", id: "meeting-1" },
      { kind: "transcript", id: "/save/transcript.txt" },
      { kind: "report_run", id: "run-1" },
      { kind: "legacy_report", id: "report-1" },
      { kind: "meeting", id: "meeting-1" },
    ];

    const hidden = applyListVisibilityRequest(initial, {
      targets,
      hidden: true,
    });

    expect(initial).toEqual(emptyListVisibilityState());
    expect(hidden).toEqual({
      meetings: ["meeting-1"],
      transcripts: ["/save/transcript.txt"],
      report_runs: ["run-1"],
      legacy_reports: ["report-1"],
    });
    expect(
      applyListVisibilityRequest(hidden, {
        targets,
        hidden: false,
      }),
    ).toEqual(emptyListVisibilityState());
  });

  it("projects items into stable visible and hidden collections", () => {
    const items = [
      { id: "meeting-1", title: "Hidden" },
      { id: "meeting-2", title: "Visible" },
    ];
    const state = {
      ...emptyListVisibilityState(),
      meetings: ["meeting-1"],
    };

    expect(
      projectListItems(items, state, (item) => ({
        kind: "meeting",
        id: item.id,
      })),
    ).toEqual({
      visible: [items[1]],
      hidden: [items[0]],
    });
  });

  it("keeps report identity on its source run after completion", () => {
    const sourceRun = reportRun({
      status: "completed",
      stage: "completed",
      report_id: "report-1",
    });

    expect(reportListVisibilityTarget(runEntry(sourceRun))).toEqual({
      kind: "report_run",
      id: "run-1",
    });
    expect(
      reportListVisibilityTarget(completedReportEntry(sourceRun)),
    ).toEqual({
      kind: "report_run",
      id: "run-1",
    });
    expect(
      reportListVisibilityTarget(completedReportEntry(null)),
    ).toEqual({
      kind: "legacy_report",
      id: "report-1",
    });
  });
});
