import type { LlmReportKind, TranscriptDay } from "./types";

/**
 * Selection state and validation for composing a new report.
 *
 * Kept free of React and of the Tauri client so the rules that decide whether a
 * report can be generated — and what the user is about to spend tokens on — are
 * assertable on their own.
 */

export const REPORT_KINDS: ReadonlyArray<{
  kind: LlmReportKind;
  label: string;
  description: string;
}> = [
  {
    kind: "summary",
    label: "Summary",
    description:
      "A short read: one-line summary, key points, decisions, action items, open questions.",
  },
  {
    kind: "detailed_report",
    label: "Detailed report",
    description:
      "A full write-up with chronological detail, risks, and evidence notes. Uses more iterations, so it costs more.",
  },
];

export function reportKindLabel(kind: LlmReportKind): string {
  return kind === "summary" ? "Summary" : "Detailed report";
}

/** Days newest-first, which is the order a user scans them in. */
export function sortDaysDescending(
  days: readonly TranscriptDay[],
): TranscriptDay[] {
  return [...days].sort((left, right) => right.day.localeCompare(left.day));
}

export function selectedCaptureCount(
  days: readonly TranscriptDay[],
  selectedDays: ReadonlySet<string>,
): number {
  return days.reduce(
    (total, day) => (selectedDays.has(day.day) ? total + day.count : total),
    0,
  );
}

/** Inclusive day span of the selection, or null when nothing is selected. */
export function selectedDateRange(
  selectedDays: ReadonlySet<string>,
): { start: string; end: string } | null {
  const sorted = [...selectedDays].sort();
  if (sorted.length === 0) {
    return null;
  }
  return { start: sorted[0], end: sorted[sorted.length - 1] };
}

export function describeReportScope(
  days: readonly TranscriptDay[],
  selectedDays: ReadonlySet<string>,
): string {
  const captures = selectedCaptureCount(days, selectedDays);
  if (captures === 0) {
    return "No captures selected";
  }
  const dayCount = days.filter((day) => selectedDays.has(day.day)).length;
  const captureLabel = `${captures} capture${captures === 1 ? "" : "s"}`;
  const range = selectedDateRange(selectedDays);
  if (dayCount === 1) {
    return `${captureLabel} from ${range?.start}`;
  }
  return `${captureLabel} across ${dayCount} days · ${range?.start} → ${range?.end}`;
}

export interface ReportComposerGate {
  openrouterKeyConfigured: boolean;
  hasActiveRun: boolean;
  selectedCaptureCount: number;
  submitting: boolean;
}

/**
 * Why generation is blocked, or null when it can proceed. Order matters: the
 * most fundamental blocker wins so the user fixes causes before symptoms.
 */
export function reportComposerBlockedReason(
  gate: ReportComposerGate,
): string | null {
  if (!gate.openrouterKeyConfigured) {
    return "Add an OpenRouter API key in Settings › Integrations to generate reports.";
  }
  if (gate.hasActiveRun) {
    return "A report is already being generated. Wait for it to finish or stop it first.";
  }
  if (gate.selectedCaptureCount === 0) {
    return "Select at least one day of captures to report on.";
  }
  if (gate.submitting) {
    return "Starting the report…";
  }
  return null;
}

export function canGenerateReport(gate: ReportComposerGate): boolean {
  return reportComposerBlockedReason(gate) === null;
}

export function toggleDaySelection(
  selectedDays: ReadonlySet<string>,
  day: string,
): Set<string> {
  const next = new Set(selectedDays);
  if (next.has(day)) {
    next.delete(day);
  } else {
    next.add(day);
  }
  return next;
}

/** Preselects the most recent day so the dialog opens ready to generate. */
export function defaultSelectedDays(
  days: readonly TranscriptDay[],
): Set<string> {
  const newest = sortDaysDescending(days)[0];
  return new Set(newest ? [newest.day] : []);
}
