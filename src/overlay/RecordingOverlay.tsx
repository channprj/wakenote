import type { CSSProperties } from "react";
import { useEffect, useState } from "react";

export type OverlayCaptionPhase = "idle" | "partial" | "refining" | "final";
export type FloatingOverlayPosition = "off" | "top" | "bottom";
export type DictationOverlayState =
  | "hidden"
  | "recording"
  | "transcribing"
  | "error";

export interface DictationOverlaySnapshot {
  state: DictationOverlayState;
  message: string | null;
  style: DictationOverlayStyle;
}

export interface DictationOverlayStyle {
  background_color: string;
  background_opacity: number;
}

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
    background_color: "#000000",
    background_opacity: 82,
  };
}

function defaultDictationStyle(): DictationOverlayStyle {
  return {
    background_color: "#000000",
    background_opacity: 88,
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
    return "0 0 0";
  }
  const value = match[1];
  return [
    Number.parseInt(value.slice(0, 2), 16),
    Number.parseInt(value.slice(2, 4), 16),
    Number.parseInt(value.slice(4, 6), 16),
  ].join(" ");
}

function rgbaColor(hex: string, opacity: number): string {
  return `rgba(${hexToRgbTriplet(hex).replaceAll(" ", ", ")}, ${opacity})`;
}

function captionStyleVariables(style: OverlayCaptionStyle): CSSProperties {
  const fontSize = Math.max(18, Math.min(48, Math.round(style.font_size_px)));
  const opacity = Math.max(0, Math.min(100, Math.round(style.background_opacity))) / 100;
  return {
    "--overlay-caption-font-size": `${fontSize}px`,
    "--overlay-caption-text-color": style.text_color,
    "--overlay-caption-background-rgb": hexToRgbTriplet(style.background_color),
    "--overlay-caption-background-alpha": `${opacity}`,
    backgroundColor: rgbaColor(style.background_color, opacity),
  } as CSSProperties;
}

function emptyDictationSnapshot(): DictationOverlaySnapshot {
  return {
    state: "hidden",
    message: null,
    style: defaultDictationStyle(),
  };
}

function dictationSnapshotFromPayload(
  payload: unknown,
): DictationOverlaySnapshot {
  const data = payload as Partial<DictationOverlaySnapshot> | null;
  const state = data?.state;
  return {
    state:
      state === "recording" ||
      state === "transcribing" ||
      state === "error"
        ? state
        : "hidden",
    message: typeof data?.message === "string" ? data.message : null,
    style: {
      ...defaultDictationStyle(),
      ...(data?.style ?? {}),
    },
  };
}

function dictationStyleVariables(style: DictationOverlayStyle): CSSProperties {
  const backgroundColor = style.background_color === "#ffffff" ? "#ffffff" : "#000000";
  const opacity = Math.max(0, Math.min(100, Math.round(style.background_opacity))) / 100;
  return {
    "--dictation-background-rgb": hexToRgbTriplet(backgroundColor),
    "--dictation-background-opacity": `${opacity}`,
    "--dictation-foreground-rgb":
      backgroundColor === "#ffffff" ? "0 0 0" : "255 255 255",
    backgroundColor: rgbaColor(backgroundColor, opacity),
  } as CSSProperties;
}

function levelsFromPayload(payload: unknown): number[] {
  const levels = (payload as { levels?: unknown } | null)?.levels;
  if (!Array.isArray(levels)) {
    return [];
  }
  return levels.map((level) =>
    typeof level === "number" && Number.isFinite(level)
      ? Math.max(0, Math.min(1, level))
      : 0,
  );
}

function formatElapsed(elapsedSeconds: number): string {
  const seconds = Math.max(0, Math.floor(elapsedSeconds));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

export function OverlayContent({
  caption,
  dictation,
  levels,
  elapsedSeconds,
}: {
  caption: OverlayCaptionSnapshot;
  dictation: DictationOverlaySnapshot;
  levels: number[];
  elapsedSeconds: number;
}) {
  if (dictation.state === "recording") {
    return (
      <div
        className="overlay-dictation"
        data-dictation-state="recording"
        role="status"
        aria-live="polite"
        style={dictationStyleVariables(dictation.style)}
      >
        <span className="overlay-dictation__dot" aria-hidden="true" />
        <span className="overlay-dictation__waveform" aria-hidden="true">
          {levels.map((level, index) => (
            <i
              key={index}
              style={{ "--level": level } as CSSProperties}
            />
          ))}
        </span>
        <span className="overlay-dictation__elapsed">
          {formatElapsed(elapsedSeconds)}
        </span>
      </div>
    );
  }

  if (dictation.state === "transcribing") {
    return (
      <div
        className="overlay-dictation"
        data-dictation-state="transcribing"
        role="status"
        aria-live="polite"
        style={dictationStyleVariables(dictation.style)}
      >
        <span className="overlay-dictation__spinner" aria-hidden="true" />
        <span className="overlay-dictation__message">
          {dictation.message ?? "Transcribing…"}
        </span>
      </div>
    );
  }

  if (dictation.state === "error") {
    return (
      <div
        className="overlay-dictation"
        data-dictation-state="error"
        role="status"
        aria-live="assertive"
        style={dictationStyleVariables(dictation.style)}
      >
        <span className="overlay-dictation__dot" aria-hidden="true" />
        <span className="overlay-dictation__message">
          {dictation.message ?? "Dictation failed"}
        </span>
      </div>
    );
  }

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
  const [dictation, setDictation] = useState<DictationOverlaySnapshot>(() =>
    emptyDictationSnapshot(),
  );
  const [levels, setLevels] = useState<number[]>(() => Array(11).fill(0));
  const [recordingStartedAt, setRecordingStartedAt] = useState<number | null>(
    null,
  );
  const [elapsedSeconds, setElapsedSeconds] = useState(0);

  useEffect(() => {
    if (dictation.state !== "recording" || recordingStartedAt === null) {
      return;
    }
    const updateElapsed = () => {
      setElapsedSeconds((Date.now() - recordingStartedAt) / 1_000);
    };
    updateElapsed();
    const timer = window.setInterval(updateElapsed, 250);
    return () => window.clearInterval(timer);
  }, [dictation.state, recordingStartedAt]);

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

      const unlistenDictation = await listen(
        "dictation-overlay-state",
        (event) => {
          const next = dictationSnapshotFromPayload(event.payload);
          setDictation(next);
          if (next.state === "recording") {
            setRecordingStartedAt(Date.now());
            setElapsedSeconds(0);
            setLevels(Array(11).fill(0));
          } else {
            setRecordingStartedAt(null);
          }
        },
      );
      if (cancelled) {
        unlistenDictation();
      } else {
        unlisteners.push(unlistenDictation);
      }

      const unlistenLevels = await listen(
        "dictation-overlay-level",
        (event) => {
          if (!cancelled) {
            setLevels(levelsFromPayload(event.payload));
          }
        },
      );
      if (cancelled) {
        unlistenLevels();
      } else {
        unlisteners.push(unlistenLevels);
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

  return (
    <OverlayContent
      caption={caption}
      dictation={dictation}
      levels={levels}
      elapsedSeconds={elapsedSeconds}
    />
  );
}
