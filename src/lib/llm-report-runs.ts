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

/**
 * Text a report entry can be matched against.
 *
 * Report bodies are not in the history listing (each would need its own detail
 * load), so filtering covers what the list already knows: kind, file name,
 * model, covered dates, and the run's own status message.
 */
export function reportEntrySearchText(entry: ReportListEntry): string {
  const kindLabel = entry.kind === "summary" ? "summary" : "detailed report";
  const parts = [
    kindLabel,
    entry.createdAt,
    entry.report?.file_name,
    entry.report?.model,
    entry.report?.date_range,
    entry.run?.message,
    entry.run?.status,
    entry.run?.model,
    entry.run?.date_range,
  ];
  return parts.filter(Boolean).join(" ").toLowerCase();
}

/** Case-insensitive match on every whitespace-separated term. */
export function filterReportEntries(
  entries: readonly ReportListEntry[],
  query: string,
): ReportListEntry[] {
  const terms = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) {
    return [...entries];
  }
  return entries.filter((entry) => {
    const haystack = reportEntrySearchText(entry);
    return terms.every((term) => haystack.includes(term));
  });
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
