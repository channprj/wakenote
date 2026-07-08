import type { CSSProperties } from "react";
import { useEffect, useState } from "react";

export type OverlayCaptionPhase = "idle" | "partial" | "refining" | "final";
export type FloatingOverlayPosition = "off" | "top" | "bottom";

export interface OverlayCaptionSnapshot {
  generation: number;
  visible: boolean;
  phase: OverlayCaptionPhase;
  chunk_id: number | null;
  audio_path: string | null;
  text: string;
  position: FloatingOverlayPosition;
  final_hold_ms: number | null;
  style: OverlayCaptionStyle;
}

export interface OverlayCaptionStyle {
  font_size_px: number;
  text_color: string;
  background_color: string;
  background_opacity: number;
}

export function emptyCaptionSnapshot(): OverlayCaptionSnapshot {
  return {
    generation: 0,
    visible: false,
    phase: "idle",
    chunk_id: null,
    audio_path: null,
    text: "",
    position: "off",
    final_hold_ms: null,
    style: defaultCaptionStyle(),
  };
}

function defaultCaptionStyle(): OverlayCaptionStyle {
  return {
    font_size_px: 24,
    text_color: "#ffffff",
    background_color: "#050507",
    background_opacity: 82,
  };
}

function snapshotFromPayload(payload: unknown): OverlayCaptionSnapshot {
  const data = payload as Partial<OverlayCaptionSnapshot> | null;
  return {
    ...emptyCaptionSnapshot(),
    ...(data ?? {}),
    style: {
      ...defaultCaptionStyle(),
      ...(data?.style ?? {}),
    },
  };
}

function hexToRgbTriplet(hex: string): string {
  const normalized = hex.trim();
  const match = normalized.match(/^#?([0-9a-fA-F]{6})$/);
  if (!match) {
    return "5 5 7";
  }
  const value = match[1];
  return [
    Number.parseInt(value.slice(0, 2), 16),
    Number.parseInt(value.slice(2, 4), 16),
    Number.parseInt(value.slice(4, 6), 16),
  ].join(" ");
}

function captionStyleVariables(style: OverlayCaptionStyle): CSSProperties {
  const fontSize = Math.max(18, Math.min(48, Math.round(style.font_size_px)));
  const opacity = Math.max(0, Math.min(100, Math.round(style.background_opacity))) / 100;
  return {
    "--overlay-caption-font-size": `${fontSize}px`,
    "--overlay-caption-text-color": style.text_color,
    "--overlay-caption-background-rgb": hexToRgbTriplet(style.background_color),
    "--overlay-caption-background-alpha": `${opacity}`,
  } as CSSProperties;
}

export function OverlayContent({ caption }: { caption: OverlayCaptionSnapshot }) {
  if (!caption.visible || !caption.text) {
    return null;
  }

  return (
    <div
      className="overlay-caption"
      data-status={caption.phase}
      role="status"
      aria-live="polite"
      style={captionStyleVariables(caption.style)}
    >
      <span className="overlay-caption__text">{caption.text}</span>
    </div>
  );
}

export function RecordingOverlay() {
  const [caption, setCaption] = useState<OverlayCaptionSnapshot>(() => emptyCaptionSnapshot());

  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");

      for (const eventName of ["overlay-caption-updated", "overlay-caption-hidden"]) {
        const unlisten = await listen(eventName, (event) => {
          setCaption(snapshotFromPayload(event.payload));
        });
        if (cancelled) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      }

      try {
        const snapshot = await invoke<OverlayCaptionSnapshot>("overlay_caption_snapshot");
        if (!cancelled) {
          setCaption(snapshotFromPayload(snapshot));
        }
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn("[wakenote overlay] snapshot failed:", err);
      }
    })();

    return () => {
      cancelled = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  return <OverlayContent caption={caption} />;
}
