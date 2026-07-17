import type {
  LlmReportHistoryItem,
  LlmReportKind,
  LlmReportRunSnapshot,
} from "./types";

export type ReportListEntry =
  | {
      key: `run:${string}`;
      kind: LlmReportKind;
      createdAt: string;
      run: LlmReportRunSnapshot;
      report: null;
      sourceRun: LlmReportRunSnapshot;
    }
  | {
      key: `report:${string}`;
      kind: LlmReportKind;
      createdAt: string;
      run: null;
      report: LlmReportHistoryItem;
      sourceRun: LlmReportRunSnapshot | null;
    };

export function isActiveLlmReportRun(
  run: LlmReportRunSnapshot,
): boolean {
  return ["queued", "running", "stopping"].includes(run.status);
}

export function mergeLlmReportRun(
  current: readonly LlmReportRunSnapshot[],
  incoming: LlmReportRunSnapshot,
): LlmReportRunSnapshot[] {
  const existing = current.find(
    (run) => run.run_id === incoming.run_id,
  );
  if (existing && existing.revision >= incoming.revision) {
    return [...current];
  }
  return [
    incoming,
    ...current.filter((run) => run.run_id !== incoming.run_id),
  ].sort((left, right) =>
    right.created_at.localeCompare(left.created_at),
  );
}

export function combineReportEntries(
  runs: readonly LlmReportRunSnapshot[],
  reports: readonly LlmReportHistoryItem[],
): ReportListEntry[] {
  const runsByReport = new Map(
    runs
      .filter((run) => run.report_id)
      .map((run) => [run.report_id as string, run]),
  );
  const reportIds = new Set(
    reports.map((report) => report.report_id),
  );
  const runEntries: ReportListEntry[] = runs
    .filter(
      (run) => !(run.report_id && reportIds.has(run.report_id)),
    )
    .map((run) => ({
      key: `run:${run.run_id}`,
      kind: run.kind,
      createdAt: run.created_at,
      run,
      report: null,
      sourceRun: run,
    }));
  const reportEntries: ReportListEntry[] = reports.map((report) => ({
    key: `report:${report.report_id}`,
    kind: report.kind,
    createdAt: report.created_at,
    run: null,
    report,
    sourceRun: runsByReport.get(report.report_id) ?? null,
  }));
  return [...runEntries, ...reportEntries].sort((left, right) =>
    right.createdAt.localeCompare(left.createdAt),
  );
}
