import {
  useCallback,
  useEffect,
  useRef,
  useState,
} from "react";
import {
  loadListVisibility,
  setListVisibility,
} from "@/lib/tauri-client";
import { emptyListVisibilityState } from "@/lib/list-visibility";
import type {
  ListVisibilityState,
  ListVisibilityTarget,
} from "@/lib/types";

export interface UseListVisibilityResult {
  state: ListVisibilityState;
  loading: boolean;
  mutating: boolean;
  error: string | null;
  announcement: string;
  setTargetsHidden(
    targets: ListVisibilityTarget[],
    hidden: boolean,
  ): Promise<boolean>;
  reload(): Promise<void>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useListVisibility(): UseListVisibilityResult {
  const [state, setState] = useState<ListVisibilityState>(
    emptyListVisibilityState,
  );
  const [loading, setLoading] = useState(true);
  const [mutating, setMutating] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const mountedRef = useRef(true);
  const mutationPendingRef = useRef(false);

  const reload = useCallback(async () => {
    if (mountedRef.current) {
      setLoading(true);
      setError(null);
    }
    try {
      const next = await loadListVisibility();
      if (mountedRef.current) {
        setState(next);
      }
    } catch (cause) {
      if (mountedRef.current) {
        setState(emptyListVisibilityState());
        setError(errorMessage(cause));
      }
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void reload();
    return () => {
      mountedRef.current = false;
    };
  }, [reload]);

  const setTargetsHidden = useCallback(
    async (
      targets: ListVisibilityTarget[],
      hidden: boolean,
    ): Promise<boolean> => {
      if (targets.length === 0 || mutationPendingRef.current) {
        return false;
      }
      mutationPendingRef.current = true;
      if (mountedRef.current) {
        setMutating(true);
        setError(null);
        setAnnouncement("");
      }
      try {
        const next = await setListVisibility({ targets, hidden });
        if (mountedRef.current) {
          setState(next);
          setAnnouncement(
            hidden
              ? "Hidden from list · Files remain on disk"
              : "Restored to list",
          );
        }
        return true;
      } catch (cause) {
        if (mountedRef.current) {
          setError(errorMessage(cause));
        }
        return false;
      } finally {
        mutationPendingRef.current = false;
        if (mountedRef.current) {
          setMutating(false);
        }
      }
    },
    [],
  );

  return {
    state,
    loading,
    mutating,
    error,
    announcement,
    setTargetsHidden,
    reload,
  };
}
