import { Loader2, Mic, X } from "lucide-react";
import { useEffect, useState } from "react";

export type OverlayState = "hidden" | "recording" | "transcribing";

interface OverlayStatePayload {
  state: OverlayState;
  position: "off" | "top" | "bottom";
}

const WAVEFORM_BAR_COUNT = 9;

async function stopLiveCapture() {
  if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    await invoke("stop_live_capture");
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn("[sagwan overlay] stop_live_capture failed:", err);
  }
}

export function OverlayContent({
  state,
  onStop,
}: {
  state: OverlayState;
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

  // Recording
  // TODO(M5c): replace placeholder pulse with real `live-level` event plumbing.
  return (
    <div className="overlay-pill" data-state="recording" role="status" aria-live="polite">
      <span className="overlay-pill__leading overlay-pill__leading--mic" aria-hidden="true">
        <Mic />
      </span>
      <span className="overlay-pill__middle">
        <span className="overlay-waveform" aria-hidden="true">
          {Array.from({ length: WAVEFORM_BAR_COUNT }, (_, i) => (
            <span key={i} style={{ animationDelay: `${i * 80}ms` }} />
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

  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    let unlisten: (() => void) | null = null;

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const handle = await listen<OverlayStatePayload>("overlay-state", (event) => {
        if (event.payload && typeof event.payload.state === "string") {
          setState(event.payload.state);
        }
      });
      if (cancelled) {
        handle();
      } else {
        unlisten = handle;
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  return <OverlayContent state={state} onStop={() => void stopLiveCapture()} />;
}
