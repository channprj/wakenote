import { useCallback, useEffect, useMemo, useReducer } from "react";
import {
  listLlmReportRuns,
  subscribeLlmReportRuns,
} from "@/lib/tauri-client";
import { isActiveLlmReportRun, mergeLlmReportRun } from "@/lib/llm-report-runs";
import type { LlmReportRunSnapshot } from "@/lib/types";

export interface LlmReportRunsState {
  runs: LlmReportRunSnapshot[];
  loading: boolean;
  error: string | null;
}

export type LlmReportRunsAction =
  | { type: "loading" }
  | { type: "hydrate"; runs: LlmReportRunSnapshot[] }
  | { type: "update"; run: LlmReportRunSnapshot }
  | { type: "ready" }
  | { type: "failed"; error: string };

const initialLlmReportRunsState: LlmReportRunsState = {
  runs: [],
  loading: true,
  error: null,
};

export function llmReportRunsReducer(
  state: LlmReportRunsState,
  action: LlmReportRunsAction,
): LlmReportRunsState {
  switch (action.type) {
    case "loading":
      return { ...state, loading: true, error: null };
    case "hydrate":
      return {
        ...state,
        runs: action.runs.reduce(
          (runs, incoming) => mergeLlmReportRun(runs, incoming),
          state.runs,
        ),
      };
    case "update":
      return {
        ...state,
        runs: mergeLlmReportRun(state.runs, action.run),
      };
    case "ready":
      return { ...state, loading: false };
    case "failed":
      return { ...state, loading: false, error: action.error };
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function useLlmReportRuns() {
  const [state, dispatch] = useReducer(
    llmReportRunsReducer,
    initialLlmReportRunsState,
  );

  const refresh = useCallback(async () => {
    dispatch({ type: "loading" });
    try {
      const runs = await listLlmReportRuns();
      dispatch({ type: "hydrate", runs });
      dispatch({ type: "ready" });
    } catch (error) {
      dispatch({ type: "failed", error: errorMessage(error) });
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void subscribeLlmReportRuns((run) => {
      if (!disposed) {
        dispatch({ type: "update", run });
      }
    })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
          return null;
        }
        unlisten = stopListening;
        return listLlmReportRuns();
      })
      .then((runs) => {
        if (disposed || runs === null) {
          return;
        }
        dispatch({ type: "hydrate", runs });
        dispatch({ type: "ready" });
      })
      .catch((error: unknown) => {
        if (!disposed) {
          dispatch({ type: "failed", error: errorMessage(error) });
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const activeRun = useMemo(
    () => state.runs.find(isActiveLlmReportRun) ?? null,
    [state.runs],
  );

  return {
    ...state,
    activeRun,
    refresh,
  };
}
