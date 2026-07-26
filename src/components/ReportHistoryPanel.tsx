import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useLlmReportRuns } from "../hooks/use-llm-report-runs";
import { useListVisibility } from "../hooks/use-list-visibility";
import {
  projectListItems,
  reportListVisibilityTarget,
} from "../lib/list-visibility";
import {
  combineReportEntries,
  type ReportListEntry,
} from "../lib/llm-report-runs";
import {
  cancelLlmReport,
  downloadLlmReport,
  listLlmReportHistory,
  loadLlmReportHistoryDetail,
  loadTranscriptDays,
  loadTranscriptsForDay,
  retryLlmReport,
  startLlmReport,
} from "../lib/tauri-client";
import type {
  LlmReportHistoryDetail,
  LlmReportHistoryItem,
  LlmReportKind,
  LlmReportRunSnapshot,
  TranscriptDay,
} from "../lib/types";
import { ReportComposer } from "./reports/ReportComposer";
import { ReportHistoryView } from "./ReportHistoryView";

interface ReportHistorySelection {
  selectedKey: string | null;
  detailReloadRevision: number;
}

export function reportSelectionAfterRefresh(
  current: ReportHistorySelection,
  entries: readonly ReportListEntry[],
): ReportHistorySelection {
  let selectedKey = current.selectedKey;
  if (
    selectedKey &&
    !entries.some((entry) => entry.key === selectedKey)
  ) {
    const selectedRunId = selectedKey.startsWith("run:")
      ? selectedKey.slice("run:".length)
      : null;
    selectedKey = selectedRunId
      ? entries.find(
          (entry) => entry.sourceRun?.run_id === selectedRunId,
        )?.key ?? null
      : null;
  }
  return {
    selectedKey: selectedKey ?? entries[0]?.key ?? null,
    detailReloadRevision: current.detailReloadRevision + 1,
  };
}

export function ReportHistoryPanel({
  model,
  maxIterations,
  openrouterKeyConfigured,
  onOpenIntegrationSettings,
}: {
  model: string;
  maxIterations: number;
  openrouterKeyConfigured: boolean;
  onOpenIntegrationSettings?: () => void;
}) {
  const {
    runs,
    activeRun,
    loading: runsLoading,
    error: runsError,
    refresh: refreshRuns,
  } = useLlmReportRuns();
  const [composerOpen, setComposerOpen] = useState(false);
  const [days, setDays] = useState<TranscriptDay[]>([]);
  const [daysLoading, setDaysLoading] = useState(false);
  const [daysError, setDaysError] = useState<string | null>(null);
  const [composerError, setComposerError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [items, setItems] = useState<LlmReportHistoryItem[]>([]);
  const [selection, setSelection] = useState<ReportHistorySelection>({
    selectedKey: null,
    detailReloadRevision: 0,
  });
  const [detail, setDetail] = useState<LlmReportHistoryDetail | null>(
    null,
  );
  const [historyLoading, setHistoryLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [downloadingId, setDownloadingId] = useState<string | null>(
    null,
  );
  const [actionPendingRunId, setActionPendingRunId] = useState<
    string | null
  >(null);
  const [error, setError] = useState<string | null>(null);
  const [visibilityMode, setVisibilityMode] = useState<
    "visible" | "hidden"
  >("visible");
  const visibility = useListVisibility();
  const completedRevisions = useRef(new Map<string, number>());

  const allEntries = useMemo(
    () => combineReportEntries(runs, items),
    [items, runs],
  );
  const projectedEntries = useMemo(
    () =>
      projectListItems(
        allEntries,
        visibility.state,
        reportListVisibilityTarget,
      ),
    [allEntries, visibility.state],
  );
  const entries =
    visibilityMode === "visible"
      ? projectedEntries.visible
      : projectedEntries.hidden;

  const refreshHistory = useCallback(async () => {
    setHistoryLoading(true);
    setError(null);
    try {
      const nextItems = await listLlmReportHistory();
      setItems(nextItems);
      if (nextItems.length === 0) {
        setDetail(null);
      }
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setHistoryLoading(false);
    }
  }, []);

  const refresh = useCallback(async () => {
    await Promise.all([refreshHistory(), refreshRuns()]);
  }, [refreshHistory, refreshRuns]);

  useEffect(() => {
    void refreshHistory();
  }, [refreshHistory]);

  useEffect(() => {
    setSelection((current) => {
      if (
        current.selectedKey &&
        entries.some((entry) => entry.key === current.selectedKey)
      ) {
        return current;
      }
      return reportSelectionAfterRefresh(current, entries);
    });
  }, [entries]);

  useEffect(() => {
    let shouldRefresh = false;
    for (const run of runs) {
      if (run.status !== "completed" || !run.report_id) {
        continue;
      }
      const handledRevision = completedRevisions.current.get(run.run_id);
      if (
        handledRevision === undefined ||
        run.revision > handledRevision
      ) {
        completedRevisions.current.set(run.run_id, run.revision);
        shouldRefresh = true;
      }
    }
    if (shouldRefresh) {
      void refreshHistory();
    }
  }, [refreshHistory, runs]);

  const selectedReportId =
    selection.selectedKey?.startsWith("report:")
      ? selection.selectedKey.slice("report:".length)
      : null;

  useEffect(() => {
    if (!selectedReportId) {
      setDetail(null);
      setDetailLoading(false);
      return;
    }
    let cancelled = false;
    setDetailLoading(true);
    setError(null);
    void loadLlmReportHistoryDetail(selectedReportId)
      .then((nextDetail) => {
        if (!cancelled) setDetail(nextDetail);
      })
      .catch((caught) => {
        if (!cancelled) setError(errorMessage(caught));
      })
      .finally(() => {
        if (!cancelled) setDetailLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedReportId, selection.detailReloadRevision]);

  const download = useCallback(
    async (reportId: string, fileName: string) => {
      setDownloadingId(reportId);
      setError(null);
      try {
        await downloadLlmReport(reportId, fileName);
      } catch (caught) {
        setError(errorMessage(caught));
      } finally {
        setDownloadingId(null);
      }
    },
    [],
  );

  const cancel = useCallback(async (runId: string) => {
    setActionPendingRunId(runId);
    setError(null);
    try {
      await cancelLlmReport(runId);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setActionPendingRunId(null);
    }
  }, []);

  const openComposer = useCallback(() => {
    setComposerError(null);
    setComposerOpen(true);
    setDaysLoading(true);
    setDaysError(null);
    void loadTranscriptDays()
      .then(setDays)
      .catch((caught) => setDaysError(errorMessage(caught)))
      .finally(() => setDaysLoading(false));
  }, []);

  // The backend takes the transcripts themselves, not a date range, so the
  // selected days are expanded here before the run starts.
  const generate = useCallback(
    async (kind: LlmReportKind, selectedDays: readonly string[]) => {
      setSubmitting(true);
      setComposerError(null);
      try {
        const perDay = await Promise.all(
          selectedDays.map((day) => loadTranscriptsForDay(day)),
        );
        const transcripts = perDay.flat();
        if (transcripts.length === 0) {
          setComposerError(
            "Those days no longer contain any captures. Reload and pick again.",
          );
          return;
        }
        const started = await startLlmReport({ kind, transcripts });
        setComposerOpen(false);
        setSelection((current) => ({
          selectedKey: `run:${started.run_id}`,
          detailReloadRevision: current.detailReloadRevision,
        }));
      } catch (caught) {
        setComposerError(errorMessage(caught));
      } finally {
        setSubmitting(false);
      }
    },
    [],
  );

  const retry = useCallback(async (runId: string) => {
    setActionPendingRunId(runId);
    setError(null);
    try {
      const nextRun = await retryLlmReport(runId);
      setSelection((current) => ({
        selectedKey: `run:${nextRun.run_id}`,
        detailReloadRevision: current.detailReloadRevision,
      }));
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setActionPendingRunId(null);
    }
  }, []);

  return (
    <>
      <ReportComposer
        days={days}
        daysError={daysError}
        daysLoading={daysLoading}
        error={composerError}
        hasActiveRun={Boolean(activeRun)}
        maxIterations={maxIterations}
        model={model}
        onGenerate={(kind, selectedDays) =>
          void generate(kind, selectedDays)
        }
        onOpenChange={setComposerOpen}
        onOpenIntegrationSettings={onOpenIntegrationSettings}
        open={composerOpen}
        openrouterKeyConfigured={openrouterKeyConfigured}
        submitting={submitting}
      />
      <ReportHistoryView
        actionPendingRunId={actionPendingRunId}
        detail={detail}
        detailLoading={detailLoading}
        downloadingId={downloadingId}
        entries={entries}
        error={error ?? runsError ?? visibility.error}
        loading={historyLoading || runsLoading}
        selectedKey={selection.selectedKey}
        visibilityMode={visibilityMode}
        visibleCount={projectedEntries.visible.length}
        hiddenCount={projectedEntries.hidden.length}
        visibilityMutating={
          visibility.loading || visibility.mutating
        }
        visibilityStatus={visibility.announcement}
        onCancel={(runId) => void cancel(runId)}
        onDownload={(reportId, fileName) =>
          void download(reportId, fileName)
        }
        onRefresh={() => void refresh()}
        onRetry={(runId) => void retry(runId)}
        onNewReport={openComposer}
        onVisibilityModeChange={setVisibilityMode}
        onSetEntriesHidden={(selectedEntries, hidden) =>
          visibility.setTargetsHidden(
            selectedEntries.map(reportListVisibilityTarget),
            hidden,
          )
        }
        onSelect={(selectedKey) =>
          setSelection((current) =>
            current.selectedKey === selectedKey
              ? current
              : { ...current, selectedKey },
          )
        }
      />
    </>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
