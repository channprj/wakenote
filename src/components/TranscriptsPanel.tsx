import { useCallback, useEffect, useRef, useState } from "react";
import { loadTranscriptDays, loadTranscriptsForDay } from "../lib/tauri-client";
import type { CustomSourceEntry, RecentTranscript, TranscriptDay } from "../lib/types";
import { TranscriptsView, formatLocalDay } from "./TranscriptsView";

export function TranscriptsPanel({
  customSources = [],
}: {
  customSources?: readonly CustomSourceEntry[];
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
      void loadDay(day, true);
      void refreshDays();
    },
    [loadDay, refreshDays],
  );

  useEffect(() => {
    void refreshDays();
  }, [refreshDays]);

  // Auto-refresh today's bucket as new transcripts are finalized (live or queue).
  // Only today is reloaded (locally — today's files are freshly written, never
  // evicted); the full day list is NOT re-scanned per final, since today is
  // always selectable in the calendar regardless of its count. Re-scanning the
  // whole archive on every transcription would stat thousands of files.
  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stop = await listen("live-transcript-final", () => {
        const todayDay = formatLocalDay(new Date());
        requestedRef.current.add(todayDay);
        void loadDay(todayDay, false);
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
  }, [loadDay]);

  return (
    <TranscriptsView
      days={days}
      entriesByDay={entriesByDay}
      loadingDay={loadingDay}
      sourceLabels={sourceLabelsFromCustomSources(customSources)}
      onActiveDayChange={ensureDayLoaded}
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
