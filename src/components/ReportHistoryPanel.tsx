import { useCallback, useEffect, useState } from "react";
import {
  downloadLlmReport,
  listLlmReportHistory,
  loadLlmReportHistoryDetail,
} from "../lib/tauri-client";
import type { LlmReportHistoryDetail, LlmReportHistoryItem } from "../lib/types";
import { ReportHistoryView } from "./ReportHistoryView";

interface ReportHistorySelection {
  selectedId: string | null;
  detailReloadRevision: number;
}

export function historySelectionAfterRefresh(
  current: ReportHistorySelection,
  items: readonly LlmReportHistoryItem[],
): ReportHistorySelection {
  const selectedId =
    current.selectedId && items.some((item) => item.report_id === current.selectedId)
      ? current.selectedId
      : items[0]?.report_id ?? null;
  return {
    selectedId,
    detailReloadRevision: current.detailReloadRevision + 1,
  };
}

export function ReportHistoryPanel() {
  const [items, setItems] = useState<LlmReportHistoryItem[]>([]);
  const [selection, setSelection] = useState<ReportHistorySelection>({
    selectedId: null,
    detailReloadRevision: 0,
  });
  const [detail, setDetail] = useState<LlmReportHistoryDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [detailLoading, setDetailLoading] = useState(false);
  const [downloadingId, setDownloadingId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const nextItems = await listLlmReportHistory();
      setItems(nextItems);
      setSelection((current) => historySelectionAfterRefresh(current, nextItems));
      if (nextItems.length === 0) {
        setDetail(null);
      }
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!selection.selectedId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    setDetailLoading(true);
    setError(null);
    void loadLlmReportHistoryDetail(selection.selectedId)
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
  }, [selection.selectedId, selection.detailReloadRevision]);

  const download = useCallback(async (reportId: string, fileName: string) => {
    setDownloadingId(reportId);
    setError(null);
    try {
      await downloadLlmReport(reportId, fileName);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setDownloadingId(null);
    }
  }, []);

  return (
    <ReportHistoryView
      detail={detail}
      detailLoading={detailLoading}
      downloadingId={downloadingId}
      error={error}
      items={items}
      loading={loading}
      selectedId={selection.selectedId}
      onDownload={(reportId, fileName) => void download(reportId, fileName)}
      onRefresh={() => void refresh()}
      onSelect={(selectedId) =>
        setSelection((current) =>
          current.selectedId === selectedId ? current : { ...current, selectedId },
        )
      }
    />
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
