import { useCallback, useEffect, useMemo, useState } from "react";
import {
  loadRecognizedSources,
  loadSourceCaptureStatus,
  startSourceCapture,
  stopSourceCapture,
} from "../lib/tauri-client";
import type {
  AppSettings,
  RecognizedSourceInfo,
  SourcePayload,
} from "../lib/types";
import { Button } from "./ui/button";
import { Switch } from "./ui/switch";

const BUILTIN_SOURCES: ReadonlyArray<RecognizedSourceInfo> = [
  {
    id: "meet",
    label: "Google Meet",
    description: "Google Meet or browser tabs with Meet in the title",
    auto_prompt: true,
    title_patterns: ["google meet", "meet - "],
    custom: false,
  },
  {
    id: "zoom",
    label: "Zoom",
    description: "Zoom desktop app or windows titled Zoom Meeting",
    auto_prompt: true,
    title_patterns: ["zoom", "zoom meeting"],
    custom: false,
  },
  {
    id: "youtube",
    label: "YouTube",
    description: "YouTube tabs, videos, and YouTube Music windows",
    auto_prompt: true,
    title_patterns: ["- youtube", "youtube"],
    custom: false,
  },
];

/// Per-source auto-capture toggles plus the live detection banner
/// (pause/resume). Rendered only when the system-audio master toggle is on.
export function SystemAudioSettings({
  settings,
  onPatch,
  onPermissionRequired,
}: {
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
  onPermissionRequired: () => Promise<boolean>;
}) {
  const fallbackSources = useMemo(() => recognizedSourcesFromSettings(settings), [settings]);
  const [sources, setSources] = useState<RecognizedSourceInfo[]>(fallbackSources);
  const [detected, setDetected] = useState<SourcePayload | null>(null);
  const [capturing, setCapturing] = useState(false);
  const builtInSources = sources.filter((source) => !source.custom);

  useEffect(() => {
    setSources(fallbackSources);
  }, [fallbackSources]);

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

  const setBuiltInAutoCapture = useCallback(
    (sourceId: string, value: boolean) => {
      const next = [
        ...settings.source_auto_prompt.filter((entry) => entry.source_id !== sourceId),
        { source_id: sourceId, auto_prompt: value },
      ];
      onPatch({ source_auto_prompt: next });
      setSources((prev) =>
        prev.map((source) =>
          source.id === sourceId ? { ...source, auto_prompt: value } : source,
        ),
      );
    },
    [settings.source_auto_prompt, onPatch],
  );

  const resumeDetectedSource = useCallback(async () => {
    if (!detected || !(await onPermissionRequired())) {
      return;
    }
    await startSourceCapture(detected.source_id);
  }, [detected, onPermissionRequired]);

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
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => void stopSourceCapture()}
            >
              Pause capture
            </Button>
          ) : (
            <Button
              type="button"
              size="sm"
              onClick={() => void resumeDetectedSource()}
            >
              Resume capture
            </Button>
          )}
        </div>
      ) : null}

      <div className="system-audio-source-list">
        {builtInSources.map((source) => (
          <div className="system-audio-source" key={source.id}>
            <div className="system-audio-source__main">
              <strong>{source.label}</strong>
              <span>{source.description}</span>
            </div>
            <Switch
              aria-label={`Auto capture ${source.label}`}
              checked={source.auto_prompt}
              onCheckedChange={(value) =>
                setBuiltInAutoCapture(source.id, value)
              }
            />
          </div>
        ))}
      </div>

    </div>
  );
}

function recognizedSourcesFromSettings(settings: AppSettings): RecognizedSourceInfo[] {
  const builtIns = BUILTIN_SOURCES.map((source) => {
    const override = settings.source_auto_prompt.find((entry) => entry.source_id === source.id);
    return {
      ...source,
      auto_prompt: override ? override.auto_prompt : source.auto_prompt,
    };
  });
  return [
    ...builtIns,
    ...settings.custom_sources.map((source) => {
      const override = settings.source_auto_prompt.find((entry) => entry.source_id === source.id);
      return {
        id: source.id,
        label: source.label,
        description: source.title_patterns.join(", "),
        title_patterns: source.title_patterns,
        auto_prompt: override ? override.auto_prompt : source.auto_prompt,
        custom: true,
      };
    }),
  ];
}

export {
  customSourceText,
  parseCustomSourceInput,
} from "./settings/CustomSourceSettings";
