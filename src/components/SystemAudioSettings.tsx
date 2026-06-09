import { useCallback, useEffect, useState } from "react";
import {
  loadRecognizedSources,
  loadSourceCaptureStatus,
  startSourceCapture,
  stopSourceCapture,
} from "../lib/tauri-client";
import type { AppSettings, RecognizedSourceInfo, SourcePayload } from "../lib/types";
import { Button, Switch } from "./ui/primitives";

/// Per-source "auto-prompt on detection" toggles plus the live detection banner
/// (manual start/stop). Self-loads the recognized-source list and subscribes to
/// `source-*` events; rendered only when the system-audio master toggle is on.
export function SystemAudioSettings({
  onPatch,
}: {
  onPatch: (patch: Partial<AppSettings>) => void;
}) {
  const [sources, setSources] = useState<RecognizedSourceInfo[]>([]);
  const [detected, setDetected] = useState<SourcePayload | null>(null);
  const [capturing, setCapturing] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const [list, status] = await Promise.all([
        loadRecognizedSources(),
        loadSourceCaptureStatus(),
      ]);
      if (cancelled) return;
      setSources(list);
      setDetected(status.detected);
      setCapturing(status.capturing);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const subscriptions: Array<[string, (payload: unknown) => void]> = [
        ["source-detected", (payload) => setDetected(payload as SourcePayload)],
        ["source-ended", () => setDetected(null)],
        ["source-capture-started", () => setCapturing(true)],
        ["source-capture-stopped", () => setCapturing(false)],
        ["source-capture-error", () => setCapturing(false)],
      ];
      for (const [name, handler] of subscriptions) {
        const unlisten = await listen(name, (event) => handler(event.payload));
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

  const setAutoPrompt = useCallback(
    (sourceId: string, value: boolean) => {
      const next = sources.map((source) => ({
        source_id: source.id,
        auto_prompt: source.id === sourceId ? value : source.auto_prompt,
      }));
      onPatch({ source_auto_prompt: next });
      setSources((prev) =>
        prev.map((source) =>
          source.id === sourceId ? { ...source, auto_prompt: value } : source,
        ),
      );
    },
    [sources, onPatch],
  );

  return (
    <div className="system-audio-settings">
      {detected ? (
        <div className="system-audio-banner" aria-live="polite">
          <span>
            {capturing
              ? `Capturing ${detected.label} (${detected.app_name})`
              : `${detected.label} detected (${detected.app_name})`}
          </span>
          {capturing ? (
            <Button type="button" variant="secondary" size="sm" onClick={() => void stopSourceCapture()}>
              Stop capture
            </Button>
          ) : (
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={() => void startSourceCapture(detected.source_id)}
            >
              Start capture
            </Button>
          )}
        </div>
      ) : null}
      {sources.map((source) => (
        <div className="toggle-row" key={source.id}>
          <span>Notify when {source.label} is detected</span>
          <Switch
            label={`Notify when ${source.label} is detected`}
            checked={source.auto_prompt}
            onCheckedChange={(value) => setAutoPrompt(source.id, value)}
          />
        </div>
      ))}
    </div>
  );
}
