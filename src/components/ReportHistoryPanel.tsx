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
  retryLlmReport,
} from "../lib/tauri-client";
import type {
  LlmReportHistoryDetail,
  LlmReportHistoryItem,
  LlmReportRunSnapshot,
} from "../lib/types";
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
  onBrowseTranscripts,
}: {
  onBrowseTranscripts?: () => void;
} = {}) {
  const {
    runs,
    loading: runsLoading,
    error: runsError,
    refresh: refreshRuns,
  } = useLlmReportRuns();
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
      onBrowseTranscripts={onBrowseTranscripts}
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
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
