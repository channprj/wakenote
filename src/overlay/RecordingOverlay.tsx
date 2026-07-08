import { useEffect, useState } from "react";
import {
  hideCaptionForGeneration,
  initialCaptionState,
  reduceCaptionOverlay,
  type OverlayCaptionEvent,
  type OverlayCaptionState,
} from "./caption-state";

interface OverlayStatePayload {
  state: "hidden" | "recording" | "transcribing";
  position: "off" | "top" | "bottom";
}

async function hideOverlayWindow() {
  if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
    return;
  }
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  try {
    await getCurrentWindow().hide();
  } catch (err) {
    // eslint-disable-next-line no-console
    console.warn("[wakenote overlay] hide failed:", err);
  }
}

export function OverlayContent({ caption }: { caption: OverlayCaptionState }) {
  if (!caption.visible || !caption.text) {
    return null;
  }

  return (
    <div className="overlay-caption" data-status={caption.status} role="status" aria-live="polite">
      <span className="overlay-caption__text">{caption.text}</span>
    </div>
  );
}

export function RecordingOverlay() {
  const [caption, setCaption] = useState<OverlayCaptionState>(() => initialCaptionState());

  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const subscriptions: Array<[string, (payload: unknown) => OverlayCaptionEvent | null]> = [
        ["overlay-state", (payload) => {
          const data = payload as OverlayStatePayload;
          return data.state === "hidden" ? { type: "hidden" } : null;
        }],
        ["live-transcript-started", (payload) => {
          const data = payload as { chunk_id: number; started_at?: string };
          return { type: "started", chunk_id: data.chunk_id, started_at: data.started_at };
        }],
        ["live-transcript-partial", (payload) => {
          const data = payload as { chunk_id: number; text: string };
          return { type: "partial", chunk_id: data.chunk_id, text: data.text };
        }],
        ["live-transcript-committed", (payload) => {
          const data = payload as {
            chunk_id: number;
            audio_path: string;
            will_transcribe?: boolean;
          };
          return {
            type: "committed",
            chunk_id: data.chunk_id,
            audio_path: data.audio_path,
            will_transcribe: data.will_transcribe,
          };
        }],
        ["live-transcript-final", (payload) => {
          const data = payload as {
            chunk_id: number | null;
            audio_path: string;
            text: string;
            recorded_at?: string;
          };
          return {
            type: "final",
            chunk_id: data.chunk_id,
            audio_path: data.audio_path,
            text: data.text,
            recorded_at: data.recorded_at,
          };
        }],
        ["live-transcript-failed", (payload) => {
          const data = payload as {
            chunk_id: number | null;
            audio_path?: string;
            error?: string;
          };
          return {
            type: "failed",
            chunk_id: data.chunk_id,
            audio_path: data.audio_path,
            error: data.error,
          };
        }],
      ];

      for (const [eventName, parse] of subscriptions) {
        const unlisten = await listen(eventName, (event) => {
          const next = parse(event.payload);
          if (!next) {
            return;
          }
          setCaption((previous) => reduceCaptionOverlay(previous, next));
          if (next.type === "failed") {
            void hideOverlayWindow();
          }
        });
        if (cancelled) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      }
    })();

    return () => {
      cancelled = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  useEffect(() => {
    if (caption.hideAfterMs == null) {
      return;
    }
    const generation = caption.generation;
    const timer = window.setTimeout(() => {
      setCaption((previous) => {
        const next = hideCaptionForGeneration(previous, generation);
        if (next !== previous) {
          void hideOverlayWindow();
        }
        return next;
      });
    }, caption.hideAfterMs);

    return () => window.clearTimeout(timer);
  }, [caption.generation, caption.hideAfterMs]);

  return <OverlayContent caption={caption} />;
}
