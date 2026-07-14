import { useEffect, useReducer } from "react";
import {
  appendWaveformLevels,
  decayWaveformLevels,
} from "@/lib/waveform";

export type WaveformPhase = "idle" | "recording" | "decaying";

export interface WaveformHistoryState {
  levels: number[];
  phase: WaveformPhase;
}

export type WaveformHistoryAction =
  | { type: "levels"; levels: number[] }
  | { type: "stop" }
  | { type: "decay" };

const INITIAL_WAVEFORM_HISTORY: WaveformHistoryState = {
  levels: [],
  phase: "idle",
};

export function reduceWaveformHistory(
  state: WaveformHistoryState,
  action: WaveformHistoryAction,
): WaveformHistoryState {
  switch (action.type) {
    case "levels":
      return {
        levels: appendWaveformLevels(state.levels, action.levels),
        phase: "recording",
      };
    case "stop":
      return state.levels.some((level) => level > 0)
        ? { ...state, phase: "decaying" }
        : INITIAL_WAVEFORM_HISTORY;
    case "decay": {
      const levels = decayWaveformLevels(state.levels);
      return levels.some((level) => level > 0)
        ? { levels, phase: "decaying" }
        : INITIAL_WAVEFORM_HISTORY;
    }
  }
}

export function useWaveformHistory(recording: boolean): WaveformHistoryState {
  const [state, dispatch] = useReducer(
    reduceWaveformHistory,
    INITIAL_WAVEFORM_HISTORY,
  );

  useEffect(() => {
    if (!recording) {
      dispatch({ type: "stop" });
      return;
    }
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }

    let disposed = false;
    let unlisten: (() => void) | undefined;

    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<{ levels: number[] }>("overlay-level", (event) => {
          dispatch({ type: "levels", levels: event.payload.levels });
        }),
      )
      .then((disposeListener) => {
        if (disposed) {
          disposeListener();
        } else {
          unlisten = disposeListener;
        }
      })
      .catch((error: unknown) => {
        console.error("[wakenote FE] failed to subscribe to overlay-level", error);
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [recording]);

  useEffect(() => {
    if (recording || state.phase !== "decaying") {
      return;
    }

    const timer = window.setInterval(() => {
      dispatch({ type: "decay" });
    }, 50);
    return () => window.clearInterval(timer);
  }, [recording, state.phase]);

  return state;
}
