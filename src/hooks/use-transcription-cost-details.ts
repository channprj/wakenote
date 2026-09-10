import { useCallback, useEffect, useRef, useState } from "react";
import {
  loadTranscriptionCostDetails,
  subscribeTranscriptionCostUpdates,
} from "@/lib/tauri-client";
import type { TranscriptionCostDetails } from "@/lib/types";

export function useTranscriptionCostDetails() {
  const [details, setDetails] = useState<TranscriptionCostDetails | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const mounted = useRef(false);
  const generation = useRef(0);
  const refresh = useCallback(async () => {
    const request = ++generation.current;
    setLoading(true);
    setError(null);
    try {
      const next = await loadTranscriptionCostDetails();
      if (mounted.current && generation.current === request) setDetails(next);
    } catch {
      if (mounted.current && generation.current === request)
        setError("Could not load local API usage. Try refreshing.");
    } finally {
      if (mounted.current && generation.current === request) setLoading(false);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    let disposed = false;
    let stop: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    void refresh();
    void subscribeTranscriptionCostUpdates(() => {
      if (disposed || timer !== undefined) return;
      timer = setTimeout(() => {
        timer = undefined;
        if (!disposed) void refresh();
      }, 500);
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(() => {
        if (!disposed)
          setError(
            "Automatic updates are unavailable. Use Refresh to load the latest usage.",
          );
      });
    return () => {
      disposed = true;
      mounted.current = false;
      generation.current++;
      clearTimeout(timer);
      stop?.();
    };
  }, [refresh]);
  return { details, loading, error, refresh };
}
