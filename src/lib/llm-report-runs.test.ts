import { describe, expect, it } from "vitest";
import type {
  LlmReportHistoryItem,
  LlmReportRunSnapshot,
} from "./types";
import {
  combineReportEntries,
  filterReportEntries,
  isActiveLlmReportRun,
  mergeLlmReportRun,
  reportEntrySearchText,
} from "./llm-report-runs";

function run(
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
    progress: [{
      run_id: "run-1",
      stage: "generating",
      iteration: 1,
      max_iterations: 3,
      message: "Drafting report",
      detail: null,
    }],
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

describe("LLM report runs", () => {
  it("ignores duplicate and stale revisions", () => {
    const current = [run({ revision: 4, message: "Current" })];

    expect(mergeLlmReportRun(current, run({ revision: 3 }))).toEqual(current);
    expect(mergeLlmReportRun(current, run({ revision: 4 }))).toEqual(current);
  });

  it("recognizes every active state", () => {
    expect(isActiveLlmReportRun(run({ status: "queued" }))).toBe(true);
    expect(isActiveLlmReportRun(run({ status: "running" }))).toBe(true);
    expect(isActiveLlmReportRun(run({ status: "stopping" }))).toBe(true);
    expect(isActiveLlmReportRun(run({ status: "cancelled" }))).toBe(false);
  });

  it("deduplicates a completed run against linked history", () => {
    const completed = run({
      status: "completed",
      stage: "completed",
      report_id: "report-1",
      finished_at: "2026-07-18T00:01:00Z",
    });
    const entries = combineReportEntries([completed], [historyItem()]);

    expect(entries).toHaveLength(1);
    expect(entries[0].key).toBe("report:report-1");
    expect(entries[0].sourceRun?.run_id).toBe("run-1");
  });

  it("keeps a completed run visible until history catches up", () => {
    const completed = run({
      status: "completed",
      stage: "completed",
      report_id: "report-pending",
    });

    const entries = combineReportEntries([completed], []);

    expect(entries).toHaveLength(1);
    expect(entries[0].key).toBe("run:run-1");
  });
});

describe("report entry filtering", () => {
  const entries = combineReportEntries(
    [run({ kind: "detailed_report", message: "Refining evidence table" })],
    [
      historyItem({
        report_id: "report-alpha",
        file_name: "20260727-summary.md",
        model: "z-ai/glm-5.2",
        date_range: "2026-07-27",
      }),
      historyItem({
        report_id: "report-beta",
        kind: "detailed_report",
        file_name: "20260725-detailed-report.md",
        model: "anthropic/claude-sonnet-5",
        date_range: "2026-07-25",
        created_at: "2026-07-25T10:00:00Z",
      }),
    ],
  );

  it("indexes the kind, file name, model, and dates a list row shows", () => {
    const text = reportEntrySearchText(
      entries.find((entry) => entry.report?.report_id === "report-beta")!,
    );

    expect(text).toContain("detailed report");
    expect(text).toContain("20260725-detailed-report.md");
    expect(text).toContain("anthropic/claude-sonnet-5");
    expect(text).toContain("2026-07-25");
  });

  it("returns every entry for a blank query", () => {
    expect(filterReportEntries(entries, "")).toHaveLength(entries.length);
    expect(filterReportEntries(entries, "   ")).toHaveLength(entries.length);
  });

  it("matches on file name regardless of case", () => {
    const matched = filterReportEntries(entries, "20260725-DETAILED");

    expect(matched).toHaveLength(1);
    expect(matched[0].report?.report_id).toBe("report-beta");
  });

  it("matches on model", () => {
    const matched = filterReportEntries(entries, "claude");

    expect(matched).toHaveLength(1);
    expect(matched[0].report?.report_id).toBe("report-beta");
  });

  it("matches an in-flight run by its status message", () => {
    const matched = filterReportEntries(entries, "evidence");

    expect(matched).toHaveLength(1);
    expect(matched[0].run?.run_id).toBe("run-1");
  });

  it("requires every term to match", () => {
    expect(filterReportEntries(entries, "detailed 20260725")).toHaveLength(1);
    expect(filterReportEntries(entries, "detailed 20260727")).toHaveLength(0);
  });

  it("returns nothing when no entry matches", () => {
    expect(filterReportEntries(entries, "nonexistent")).toEqual([]);
  });

  it("does not mutate the input list", () => {
    const before = entries.length;
    filterReportEntries(entries, "claude");

    expect(entries).toHaveLength(before);
  });
});
