import { useCallback, useEffect, useState } from "react";
import { RefreshCwIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  loadTranscriptionCostSnapshot,
  subscribeTranscriptionCostUpdates,
} from "@/lib/tauri-client";
import type {
  TranscriptionCostPeriod,
  TranscriptionCostSnapshot,
} from "@/lib/types";
import { SettingsCard } from "./settings-controls";

export function TranscriptionCostDashboard() {
  const [snapshot, setSnapshot] =
    useState<TranscriptionCostSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setSnapshot(await loadTranscriptionCostSnapshot());
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void refresh();
    void subscribeTranscriptionCostUpdates((next) => {
      if (!disposed) {
        setSnapshot(next);
      }
    }).then((stop) => {
      if (disposed) {
        stop();
      } else {
        unlisten = stop;
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refresh]);

  return (
    <TranscriptionCostDashboardView
      snapshot={snapshot}
      loading={loading}
      error={error}
      onRefresh={() => void refresh()}
    />
  );
}

export function TranscriptionCostDashboardView({
  snapshot,
  loading,
  error,
  onRefresh,
}: {
  snapshot: TranscriptionCostSnapshot | null;
  loading: boolean;
  error: string | null;
  onRefresh: () => void;
}) {
  return (
    <SettingsCard
      title="Transcription API cost"
      description="Local estimates grouped by your current calendar period."
    >
      <div className="transcription-cost-dashboard">
        <div className="transcription-cost-dashboard__head">
          <span>{error ?? snapshot?.disclosure ?? "Loading local usage…"}</span>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={loading}
            onClick={onRefresh}
          >
            <RefreshCwIcon
              data-icon="inline-start"
              className={loading ? "meeting-spin" : undefined}
            />
            Refresh
          </Button>
        </div>
        <div className="transcription-cost-dashboard__periods">
          <CostPeriod label="Today" period={snapshot?.today} />
          <CostPeriod label="This week" period={snapshot?.week} />
          <CostPeriod label="This month" period={snapshot?.month} />
        </div>
      </div>
    </SettingsCard>
  );
}

function CostPeriod({
  label,
  period,
}: {
  label: string;
  period: TranscriptionCostPeriod | undefined;
}) {
  const value = period ?? {
    estimated_cost_usd: 0,
    audio_duration_ms: 0,
    request_count: 0,
    unpriced_request_count: 0,
  };
  return (
    <section className="transcription-cost-period" aria-label={label}>
      <span>{label}</span>
      <strong>${value.estimated_cost_usd.toFixed(4)}</strong>
      <small>
        {formatUsageDuration(value.audio_duration_ms)} · {value.request_count} requests
        {value.unpriced_request_count > 0
          ? ` · ${value.unpriced_request_count} unpriced`
          : ""}
      </small>
    </section>
  );
}

function formatUsageDuration(milliseconds: number) {
  const minutes = Math.round(Math.max(0, milliseconds) / 60_000);
  if (minutes < 60) {
    return `${minutes} min`;
  }
  const hours = Math.floor(minutes / 60);
  const remainder = minutes % 60;
  return remainder > 0 ? `${hours}h ${remainder}m` : `${hours}h`;
}
