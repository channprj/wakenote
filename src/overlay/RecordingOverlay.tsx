import type { CSSProperties } from "react";
import { useEffect, useRef, useState } from "react";

export type OverlayCaptionPhase = "idle" | "partial" | "refining" | "final";
export type OverlayCaptionSource =
  | "live_transcription"
  | "dictation"
  | "preview";
export type FloatingOverlayPosition =
  | "off"
  | "top_left"
  | "top"
  | "top_right"
  | "bottom_left"
  | "bottom"
  | "bottom_right";
export type SubtitleAnimation = "instant" | "fade" | "dissolve";
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
  source: OverlayCaptionSource;
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
  animation: SubtitleAnimation;
  duration_seconds: number;
  padding_horizontal_px: number;
  padding_vertical_px: number;
  border_width_px: number;
  border_color: string;
  border_radius_px: number;
  max_width_px: number;
  min_height_px: number;
  max_height_px: number;
}

export function emptyCaptionSnapshot(): OverlayCaptionSnapshot {
  return {
    generation: 0,
    visible: false,
    source: "live_transcription",
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
    animation: "fade",
    duration_seconds: 5,
    padding_horizontal_px: 18,
    padding_vertical_px: 14,
    border_width_px: 1,
    border_color: "#ffffff",
    border_radius_px: 8,
    max_width_px: 720,
    min_height_px: 58,
    max_height_px: 1_000,
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
  const source = data?.source;
  return {
    ...emptyCaptionSnapshot(),
    ...(data ?? {}),
    source:
      source === "dictation" || source === "preview"
        ? source
        : "live_transcription",
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
  const fontSize = Math.max(10, Math.min(48, Math.round(style.font_size_px)));
  const opacity =
    Math.max(0, Math.min(100, Math.round(style.background_opacity))) / 100;
  const maxWidth = Math.max(
    0,
    Math.min(1_600, Math.round(style.max_width_px)),
  );
  const minHeight = Math.max(
    0,
    Math.min(1_200, Math.round(Math.min(style.min_height_px, style.max_height_px))),
  );
  const maxHeight = Math.max(
    minHeight,
    Math.min(1_200, Math.round(Math.max(style.min_height_px, style.max_height_px))),
  );
  return {
    "--overlay-caption-font-size": `${fontSize}px`,
    "--overlay-caption-text-color": style.text_color,
    "--overlay-caption-background-rgb": hexToRgbTriplet(style.background_color),
    "--overlay-caption-background-alpha": `${opacity}`,
    "--overlay-caption-padding-horizontal": `${Math.max(0, Math.min(64, Math.round(style.padding_horizontal_px)))}px`,
    "--overlay-caption-padding-vertical": `${Math.max(0, Math.min(64, Math.round(style.padding_vertical_px)))}px`,
    "--overlay-caption-border-width": `${Math.max(0, Math.min(8, Math.round(style.border_width_px)))}px`,
    "--overlay-caption-border-rgb": hexToRgbTriplet(style.border_color),
    "--overlay-caption-border-radius": `${Math.max(0, Math.min(48, Math.round(style.border_radius_px)))}px`,
    "--overlay-caption-max-width": `${maxWidth}px`,
    "--overlay-caption-min-height": `${minHeight}px`,
    "--overlay-caption-max-height": `${maxHeight}px`,
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
      state === "recording" || state === "transcribing" || state === "error"
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
  const backgroundColor =
    style.background_color === "#ffffff" ? "#ffffff" : "#000000";
  const opacity =
    Math.max(0, Math.min(100, Math.round(style.background_opacity))) / 100;
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

function subtitleExitDurationMs(animation: SubtitleAnimation): number {
  switch (animation) {
    case "instant":
      return 0;
    case "fade":
      return 180;
    case "dissolve":
      return 240;
  }
}

export function OverlayContent({
  caption,
  dictation,
  levels,
  elapsedSeconds,
  captionExiting = false,
}: {
  caption: OverlayCaptionSnapshot;
  dictation: DictationOverlaySnapshot;
  levels: number[];
  elapsedSeconds: number;
  captionExiting?: boolean;
}) {
  const captionVisible = caption.visible && Boolean(caption.text);
  const dictationCaptionVisible =
    captionVisible && caption.source === "dictation";
  const captionContent = captionVisible ? (
    <div
      className="overlay-caption"
      data-source={caption.source}
      data-status={caption.phase}
      data-animation={caption.style.animation}
      data-visibility={captionExiting ? "exiting" : "visible"}
      role="status"
      aria-live="polite"
      style={captionStyleVariables(caption.style)}
    >
      <span className="overlay-caption__text">{caption.text}</span>
    </div>
  ) : null;

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
            <i key={index} style={{ "--level": level } as CSSProperties} />
          ))}
        </span>
        <span className="overlay-dictation__elapsed">
          {formatElapsed(elapsedSeconds)}
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

  if (dictationCaptionVisible) {
    return captionContent;
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

  if (dictation.state !== "hidden") {
    return null;
  }

  return captionContent;
}

export function RecordingOverlay() {
  const [caption, setCaption] = useState<OverlayCaptionSnapshot>(() =>
    emptyCaptionSnapshot(),
  );
  const [captionExiting, setCaptionExiting] = useState(false);
  const captionRef = useRef(caption);
  const captionExitTimerRef = useRef<number | null>(null);
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

      const applyCaptionSnapshot = (payload: unknown, hidden: boolean) => {
        const next = snapshotFromPayload(payload);
        if (captionExitTimerRef.current !== null) {
          window.clearTimeout(captionExitTimerRef.current);
          captionExitTimerRef.current = null;
        }
        const current = captionRef.current;
        const exitDuration = subtitleExitDurationMs(current.style.animation);
        if (hidden && current.visible && current.text && exitDuration > 0) {
          setCaptionExiting(true);
          captionExitTimerRef.current = window.setTimeout(() => {
            captionRef.current = next;
            setCaption(next);
            setCaptionExiting(false);
            captionExitTimerRef.current = null;
          }, exitDuration);
          return;
        }
        captionRef.current = next;
        setCaption(next);
        setCaptionExiting(false);
      };

      for (const eventName of [
        "overlay-caption-updated",
        "overlay-caption-hidden",
      ]) {
        const unlisten = await listen(eventName, (event) => {
          applyCaptionSnapshot(
            event.payload,
            eventName === "overlay-caption-hidden",
          );
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
        const snapshot = await invoke<OverlayCaptionSnapshot>(
          "overlay_caption_snapshot",
        );
        if (!cancelled) {
          applyCaptionSnapshot(snapshot, false);
        }
      } catch (err) {
        // eslint-disable-next-line no-console
        console.warn("[wakenote overlay] snapshot failed:", err);
      }
    })();

    return () => {
      cancelled = true;
      if (captionExitTimerRef.current !== null) {
        window.clearTimeout(captionExitTimerRef.current);
      }
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
      captionExiting={captionExiting}
    />
  );
}
