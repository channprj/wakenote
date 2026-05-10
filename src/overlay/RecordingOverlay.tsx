import { Loader2, Mic, X } from "lucide-react";
import type { CSSProperties } from "react";
import { useEffect, useState } from "react";

export type OverlayState = "hidden" | "recording" | "transcribing";

interface OverlayStatePayload {
  state: OverlayState;
  position: "off" | "top" | "bottom";
}

interface OverlayLevelPayload {
  levels?: number[];
}

const WAVEFORM_BAR_COUNT = 11;
const EMPTY_LEVELS = Array.from({ length: WAVEFORM_BAR_COUNT }, () => 0);

type WaveformBarStyle = CSSProperties & {
  "--bar-height": string;
  "--bar-opacity": number;
  "--bar-glow": string;
};

function clampLevel(value: number) {
  if (!Number.isFinite(value)) {
    return 0;
  }
  return Math.min(1, Math.max(0, value));
}

function waveformLevels(levels?: number[]) {
  return Array.from({ length: WAVEFORM_BAR_COUNT }, (_, index) =>
    clampLevel(levels?.[index] ?? 0),
  );
}

function smoothWaveformLevels(previous: number[], next?: number[]) {
  const current = waveformLevels(next);
  return current.map((level, index) =>
    clampLevel((previous[index] ?? 0) * 0.65 + level * 0.35),
  );
}

function waveformBarStyle(level: number): WaveformBarStyle {
  return {
    "--bar-height": `${Math.round(4 + level * 14)}px`,
    "--bar-opacity": 0.36 + level * 0.64,
    "--bar-glow": `${Math.round(2 + level * 9)}px`,
  };
}

async function stopLiveCapture() {
  if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke("stop_live_capture");
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn("[wakenote overlay] stop_live_capture failed:", err);
  }
}

export function OverlayContent({
  state,
  levels,
  onStop,
}: {
  state: OverlayState;
  levels?: number[];
  onStop: () => void;
}) {
  if (state === "hidden") {
    return null;
  }

  if (state === "transcribing") {
    return (
      <div className="overlay-pill" data-state="transcribing" role="status" aria-live="polite">
        <span className="overlay-pill__leading overlay-pill__leading--spinner" aria-hidden="true">
          <Loader2 />
        </span>
        <span className="overlay-pill__middle overlay-pill__middle--pulse">Transcribing…</span>
        <span className="overlay-pill__trailing" />
      </div>
    );
  }

  const bars = waveformLevels(levels);

  return (
    <div className="overlay-pill" data-state="recording" role="status" aria-live="polite">
      <span className="overlay-pill__leading overlay-pill__leading--mic" aria-hidden="true">
        <Mic />
      </span>
      <span className="overlay-pill__middle">
        <span className="overlay-waveform" aria-hidden="true">
          {bars.map((level, index) => (
            <span
              className="overlay-waveform__bar"
              data-peak={level >= 0.78 ? "true" : undefined}
              key={index}
              style={waveformBarStyle(level)}
            >
              <span className="overlay-waveform__bar-fill" />
            </span>
          ))}
        </span>
      </span>
      <button
        type="button"
        className="overlay-pill__trailing overlay-pill__close"
        aria-label="Stop recording"
        onClick={onStop}
      >
        <X />
      </button>
    </div>
  );
}

export function RecordingOverlay() {
  const [state, setState] = useState<OverlayState>("hidden");
  const [levels, setLevels] = useState<number[]>(EMPTY_LEVELS);

  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    let unlistenState: (() => void) | null = null;
    let unlistenLevels: (() => void) | null = null;

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const stateHandle = await listen<OverlayStatePayload>("overlay-state", (event) => {
        if (event.payload && typeof event.payload.state === "string") {
          setState(event.payload.state);
          if (event.payload.state === "hidden" || event.payload.state === "transcribing") {
            setLevels(EMPTY_LEVELS);
          }
        }
      });
      const levelHandle = await listen<OverlayLevelPayload>("overlay-level", (event) => {
        setLevels((previous) => smoothWaveformLevels(previous, event.payload?.levels));
      });
      if (cancelled) {
        stateHandle();
        levelHandle();
      } else {
        unlistenState = stateHandle;
        unlistenLevels = levelHandle;
      }
    })();

    return () => {
      cancelled = true;
      unlistenState?.();
      unlistenLevels?.();
    };
  }, []);

  return <OverlayContent state={state} levels={levels} onStop={() => void stopLiveCapture()} />;
}
