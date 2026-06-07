import { useCallback, useEffect, useRef, useState } from "react";
import { loadTranscriptDays, loadTranscriptsForDay } from "../lib/tauri-client";
import type { RecentTranscript, TranscriptDay } from "../lib/types";
import { TranscriptsView, formatLocalDay } from "./TranscriptsView";

export function TranscriptsPanel() {
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

  const loadDay = useCallback(async (day: string) => {
    setLoadingDay(day);
    try {
      const entries = await loadTranscriptsForDay(day);
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
  // reloadDay and the live-transcript-final handler intentionally bypass the
  // guard (they pre-add the day and call loadDay directly) to force a re-read.
  const ensureDayLoaded = useCallback(
    (day: string) => {
      if (requestedRef.current.has(day)) {
        return;
      }
      requestedRef.current.add(day);
      void loadDay(day);
    },
    [loadDay],
  );

  const reloadDay = useCallback(
    (day: string) => {
      requestedRef.current.add(day);
      void loadDay(day);
      void refreshDays();
    },
    [loadDay, refreshDays],
  );

  useEffect(() => {
    void refreshDays();
  }, [refreshDays]);

  // Auto-refresh today's bucket as new transcripts are finalized (live or queue).
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
        void loadDay(todayDay);
        void refreshDays();
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
      onActiveDayChange={ensureDayLoaded}
      onReload={reloadDay}
    />
  );
}
