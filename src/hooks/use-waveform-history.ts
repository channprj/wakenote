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

type RequestFrame = (callback: FrameRequestCallback) => number;
type CancelFrame = (handle: number) => void;

export interface AnimationFrameBatcher<T> {
  push: (value: T) => void;
  cancel: () => void;
}

export function createAnimationFrameBatcher<T>(
  onFlush: (value: T) => void,
  requestFrame: RequestFrame,
  cancelFrame: CancelFrame,
): AnimationFrameBatcher<T> {
  let frameHandle: number | null = null;
  let latestValue: T | undefined;

  return {
    push(value) {
      latestValue = value;
      if (frameHandle !== null) {
        return;
      }
      frameHandle = requestFrame(() => {
        frameHandle = null;
        const valueToFlush = latestValue;
        latestValue = undefined;
        if (valueToFlush !== undefined) {
          onFlush(valueToFlush);
        }
      });
    },
    cancel() {
      if (frameHandle !== null) {
        cancelFrame(frameHandle);
      }
      frameHandle = null;
      latestValue = undefined;
    },
  };
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
    const batcher = createAnimationFrameBatcher<number[]>(
      (levels) => dispatch({ type: "levels", levels }),
      window.requestAnimationFrame.bind(window),
      window.cancelAnimationFrame.bind(window),
    );

    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<{ levels: number[] }>("overlay-level", (event) => {
          batcher.push(event.payload.levels);
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
      batcher.cancel();
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
