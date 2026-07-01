import { useCallback, useEffect, useRef, useState } from "react";
import {
  loadTranscriptDays,
  loadTranscriptsForDay,
  openTranscriptFolder,
  rebuildTranscriptDayIndex,
  regenerateTranscript,
} from "../lib/tauri-client";
import { transcriptDayFromRecordingReference } from "../lib/transcript-history";
import type {
  CustomSourceEntry,
  ModelDescriptor,
  RecentTranscript,
  TranscriptDay,
} from "../lib/types";
import { TranscriptsView, formatLocalDay } from "./TranscriptsView";

export function TranscriptsPanel({
  customSources = [],
  models = [],
  selectedModelId = "",
  autoPlayNext = false,
}: {
  customSources?: readonly CustomSourceEntry[];
  models?: readonly Pick<ModelDescriptor, "id" | "display_name" | "status">[];
  selectedModelId?: string;
  autoPlayNext?: boolean;
}) {
  const [days, setDays] = useState<TranscriptDay[]>([]);
  const [entriesByDay, setEntriesByDay] = useState<Map<string, RecentTranscript[]>>(
    () => new Map(),
  );
  const [loadingDay, setLoadingDay] = useState<string | null>(null);
  const requestedRef = useRef<Set<string>>(new Set());

  const refreshDays = useCallback(async () => {
    try {
      setDays(await loadTranscriptDays());
    } catch {
      // Day listing is best-effort; the calendar still shows today.
    }
  }, []);

  // `download` is false for on-navigation loads (iCloud-evicted sidecars are
  // skipped, no network) and true for an explicit reload (evicted sidecars for
  // that day are fetched).
  const loadDay = useCallback(async (day: string, download = false) => {
    setLoadingDay(day);
    try {
      const entries = await loadTranscriptsForDay(day, download);
      setEntriesByDay((prev) => {
        const next = new Map(prev);
        next.set(day, entries);
        return next;
      });
    } catch {
      // Leave any previously loaded entries in place on failure.
    } finally {
      setLoadingDay((current) => (current === day ? null : current));
    }
  }, []);

  const rebuildDay = useCallback(async (day: string, download = true) => {
    setLoadingDay(day);
    try {
      const entries = await rebuildTranscriptDayIndex(day, download);
      setEntriesByDay((prev) => {
        const next = new Map(prev);
        next.set(day, entries);
        return next;
      });
    } catch {
      // Leave any previously loaded entries in place on failure.
    } finally {
      setLoadingDay((current) => (current === day ? null : current));
    }
  }, []);

  // TranscriptsView fires onActiveDayChange(effectiveActiveDay) on mount (with
  // today) and whenever the active day changes. ensureDayLoaded's requestedRef
  // guard is what keeps each day loaded at most once across those effect
  // re-fires — do not remove it, or navigating back and forth re-loads days.
  // First-nav loads stay local (download=false); reloadDay and the
  // live-transcript-final handler intentionally bypass the guard to force a
  // re-read (reload additionally downloads evicted sidecars).
  const ensureDayLoaded = useCallback(
    (day: string) => {
      if (requestedRef.current.has(day)) {
        return;
      }
      requestedRef.current.add(day);
      void loadDay(day, false);
    },
    [loadDay],
  );

  const reloadDay = useCallback(
    (day: string) => {
      requestedRef.current.add(day);
      void rebuildDay(day, true);
      void refreshDays();
    },
    [rebuildDay, refreshDays],
  );

  const regenerateEntries = useCallback(
    async (entries: readonly RecentTranscript[], modelId?: string) => {
      const daysToReload = new Set<string>();
      for (const entry of entries) {
        if (!entry.audio_path) {
          continue;
        }
        await regenerateTranscript(entry.audio_path, modelId);
        daysToReload.add(formatLocalDay(new Date(entry.recorded_at)));
      }
      for (const day of daysToReload) {
        requestedRef.current.add(day);
        await loadDay(day, true);
      }
      if (daysToReload.size > 0) {
        void refreshDays();
      }
    },
    [loadDay, refreshDays],
  );

  const openEntryFolder = useCallback(async (entry: RecentTranscript) => {
    await openTranscriptFolder(entry.audio_path ?? entry.transcript_path);
  }, []);

  useEffect(() => {
    void refreshDays();
  }, [refreshDays]);

  // Auto-refresh the recorded day as new transcripts are finalized (live or queue).
  // Queued jobs can finish long after recording, so the event's recorded_at
  // value, not wall-clock "today", chooses the bucket to reload.
  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen("live-transcript-final", (event) => {
        const payload = event.payload as {
          recorded_at?: string | null;
          audio_path?: string | null;
        };
        const todayDay = formatLocalDay(new Date());
        const day = transcriptDayFromRecordingReference(
          payload.recorded_at,
          payload.audio_path,
        );
        requestedRef.current.add(day);
        void loadDay(day, false);
        if (day !== todayDay) {
          void refreshDays();
        }
      });
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadDay, refreshDays]);

  return (
    <TranscriptsView
      days={days}
      entriesByDay={entriesByDay}
      loadingDay={loadingDay}
      models={models}
      selectedModelId={selectedModelId}
      sourceLabels={sourceLabelsFromCustomSources(customSources)}
      autoPlayNext={autoPlayNext}
      onActiveDayChange={ensureDayLoaded}
      onOpenFolder={openEntryFolder}
      onRegenerate={regenerateEntries}
      onReload={reloadDay}
    />
  );
}

function sourceLabelsFromCustomSources(
  customSources: readonly CustomSourceEntry[],
): Record<string, string> {
  return Object.fromEntries(
    customSources.map((source) => [source.id, source.label]),
  );
}
