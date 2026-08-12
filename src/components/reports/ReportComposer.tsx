import { AlertCircle, Loader2, SparklesIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { EmptyState } from "@/components/ui/empty-state";
import {
  REPORT_KINDS,
  canGenerateReport,
  defaultSelectedDays,
  describeReportScope,
  reportComposerBlockedReason,
  selectedCaptureCount,
  sortDaysDescending,
  toggleDaySelection,
} from "@/lib/report-composer";
import type { LlmReportKind, TranscriptDay } from "@/lib/types";

/**
 * Starts a report from the Reports page.
 *
 * Report generation previously existed only in the Transcripts selection
 * toolbar, so users looking for it where reports live found nothing. This is the
 * same operation, reached from the place it belongs.
 */
export function ReportComposer({
  open,
  onOpenChange,
  days,
  daysLoading = false,
  daysError = null,
  model,
  maxIterations,
  openrouterKeyConfigured,
  hasActiveRun,
  submitting = false,
  error = null,
  onGenerate,
  onOpenIntegrationSettings,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  days: readonly TranscriptDay[];
  daysLoading?: boolean;
  daysError?: string | null;
  model: string;
  maxIterations: number;
  openrouterKeyConfigured: boolean;
  hasActiveRun: boolean;
  submitting?: boolean;
  error?: string | null;
  onGenerate: (kind: LlmReportKind, days: readonly string[]) => void;
  onOpenIntegrationSettings?: () => void;
}) {
  const [kind, setKind] = useState<LlmReportKind>("summary");
  const [selectedDays, setSelectedDays] = useState<Set<string>>(new Set());
  const wasOpenRef = useRef(false);
  const selectionEditedRef = useRef(false);
  const orderedDays = useMemo(() => sortDaysDescending(days), [days]);

  // Reopening should offer the newest captures, not a stale selection.
  useEffect(() => {
    if (!open) {
      wasOpenRef.current = false;
      return;
    }
    if (!wasOpenRef.current) {
      wasOpenRef.current = true;
      selectionEditedRef.current = false;
      setSelectedDays(defaultSelectedDays(days));
      return;
    }

    setSelectedDays((current) => {
      if (!selectionEditedRef.current) {
        return defaultSelectedDays(days);
      }
      const availableDays = new Set(days.map((day) => day.day));
      const next = new Set(
        [...current].filter((day) => availableDays.has(day)),
      );
      return next.size === current.size ? current : next;
    });
  }, [open, days]);

  const captures = selectedCaptureCount(days, selectedDays);
  const gate = {
    openrouterKeyConfigured,
    hasActiveRun,
    selectedCaptureCount: captures,
    submitting,
  };
  const blockedReason = reportComposerBlockedReason(gate);
  const canGenerate = canGenerateReport(gate);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="report-composer"
        aria-label="New report"
        onOpenAutoFocus={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>New report</DialogTitle>
          <DialogDescription>
            Choose what to write and which captures it should cover.
          </DialogDescription>
        </DialogHeader>

        <div className="report-composer__body">
          <fieldset className="report-composer__kinds">
            <legend>Report type</legend>
            {REPORT_KINDS.map((entry) => (
              <label
                key={entry.kind}
                className="report-composer__kind"
                data-selected={kind === entry.kind}
              >
                <input
                  type="radio"
                  name="report-kind"
                  value={entry.kind}
                  checked={kind === entry.kind}
                  onChange={() => setKind(entry.kind)}
                />
                <span>
                  <strong>{entry.label}</strong>
                  <small>{entry.description}</small>
                </span>
              </label>
            ))}
          </fieldset>

          <fieldset className="report-composer__days">
            <legend>Captures to cover</legend>
            {daysError ? (
              <Alert variant="destructive">
                <AlertCircle />
                <AlertDescription>{daysError}</AlertDescription>
              </Alert>
            ) : daysLoading && orderedDays.length === 0 ? (
              <EmptyState
                icon={Loader2}
                iconClassName="loading-spin"
                title="Loading captures"
              />
            ) : orderedDays.length === 0 ? (
              <EmptyState
                icon={SparklesIcon}
                title="No captures to report on yet"
                description="Record something first — captures are grouped by day here."
              />
            ) : (
              <ul>
                {orderedDays.map((day) => (
                  <li key={day.day}>
                    <label>
                      <Checkbox
                        checked={selectedDays.has(day.day)}
                        onCheckedChange={() => {
                          selectionEditedRef.current = true;
                          setSelectedDays((current) =>
                            toggleDaySelection(current, day.day),
                          );
                        }}
                      />
                      <span>{day.day}</span>
                      <small>
                        {day.count} capture{day.count === 1 ? "" : "s"}
                      </small>
                    </label>
                  </li>
                ))}
              </ul>
            )}
          </fieldset>

          <dl className="report-composer__summary">
            <div>
              <dt>Scope</dt>
              <dd>{describeReportScope(days, selectedDays)}</dd>
            </div>
            <div>
              <dt>Model</dt>
              <dd title={model}>{model}</dd>
            </div>
            <div>
              <dt>Iteration budget</dt>
              <dd>
                Up to {maxIterations} pass{maxIterations === 1 ? "" : "es"}
              </dd>
            </div>
          </dl>

          {error ? (
            <Alert variant="destructive">
              <AlertCircle />
              <AlertDescription>{error}</AlertDescription>
            </Alert>
          ) : null}
        </div>

        <DialogFooter>
          {blockedReason ? (
            <p
              className="report-composer__blocked"
              data-tone={openrouterKeyConfigured ? "neutral" : "warning"}
            >
              {blockedReason}
              {!openrouterKeyConfigured && onOpenIntegrationSettings ? (
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  onClick={onOpenIntegrationSettings}
                >
                  Open Integrations
                </Button>
              ) : null}
            </p>
          ) : null}
          <Button
            type="button"
            size="sm"
            variant="secondary"
            onClick={() => onOpenChange(false)}
          >
            Cancel
          </Button>
          <Button
            type="button"
            size="sm"
            disabled={!canGenerate}
            title={blockedReason ?? undefined}
            onClick={() => onGenerate(kind, [...selectedDays].sort())}
          >
            {submitting ? (
              <Loader2 data-icon="inline-start" className="loading-spin" />
            ) : (
              <SparklesIcon data-icon="inline-start" />
            )}
            Generate
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
