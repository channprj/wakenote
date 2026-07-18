import type { ReportListEntry } from "./llm-report-runs";
import type {
  ListVisibilityKind,
  ListVisibilityState,
  ListVisibilityTarget,
  SetListVisibilityRequest,
} from "./types";

const FIELD_BY_KIND: Record<
  ListVisibilityKind,
  keyof ListVisibilityState
> = {
  meeting: "meetings",
  transcript: "transcripts",
  report_run: "report_runs",
  legacy_report: "legacy_reports",
};

export function emptyListVisibilityState(): ListVisibilityState {
  return {
    meetings: [],
    transcripts: [],
    report_runs: [],
    legacy_reports: [],
  };
}

export function isListVisibilityTargetHidden(
  state: ListVisibilityState,
  target: ListVisibilityTarget,
): boolean {
  return state[FIELD_BY_KIND[target.kind]].includes(target.id);
}

export function applyListVisibilityRequest(
  state: ListVisibilityState,
  request: SetListVisibilityRequest,
): ListVisibilityState {
  if (request.targets.some((target) => target.id.trim().length === 0)) {
    throw new Error("List visibility target id cannot be blank");
  }

  const next: ListVisibilityState = {
    meetings: [...state.meetings],
    transcripts: [...state.transcripts],
    report_runs: [...state.report_runs],
    legacy_reports: [...state.legacy_reports],
  };
  for (const target of request.targets) {
    const field = FIELD_BY_KIND[target.kind];
    const ids = new Set(next[field]);
    if (request.hidden) {
      ids.add(target.id);
    } else {
      ids.delete(target.id);
    }
    next[field] = [...ids].sort();
  }
  return next;
}

export function projectListItems<T>(
  items: readonly T[],
  state: ListVisibilityState,
  targetForItem: (item: T) => ListVisibilityTarget,
): { visible: T[]; hidden: T[] } {
  const visible: T[] = [];
  const hidden: T[] = [];
  for (const item of items) {
    const target = targetForItem(item);
    (isListVisibilityTargetHidden(state, target)
      ? hidden
      : visible
    ).push(item);
  }
  return { visible, hidden };
}

export function reportListVisibilityTarget(
  entry: ReportListEntry,
): ListVisibilityTarget {
  if (entry.sourceRun) {
    return {
      kind: "report_run",
      id: entry.sourceRun.run_id,
    };
  }
  if (!entry.report) {
    throw new Error(`Report list entry ${entry.key} has no stable identity`);
  }
  return {
    kind: "legacy_report",
    id: entry.report.report_id,
  };
}
