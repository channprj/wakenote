import { Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  loadRecognizedSources,
  loadSourceCaptureStatus,
  startSourceCapture,
  stopSourceCapture,
} from "../lib/tauri-client";
import type {
  AppSettings,
  CustomSourceEntry,
  RecognizedSourceInfo,
  SourcePayload,
} from "../lib/types";
import { Button, Switch } from "./ui/primitives";

const BUILTIN_SOURCES: ReadonlyArray<RecognizedSourceInfo> = [
  {
    id: "meet",
    label: "Google Meet",
    auto_prompt: true,
    title_patterns: ["google meet", "meet - "],
    custom: false,
  },
  {
    id: "youtube",
    label: "YouTube",
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
}: {
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
}) {
  const fallbackSources = useMemo(() => recognizedSourcesFromSettings(settings), [settings]);
  const [sources, setSources] = useState<RecognizedSourceInfo[]>(fallbackSources);
  const [customDrafts, setCustomDrafts] = useState<CustomSourceEntry[]>(settings.custom_sources);
  const [detected, setDetected] = useState<SourcePayload | null>(null);
  const [capturing, setCapturing] = useState(false);
  const builtInSources = sources.filter((source) => !source.custom);

  useEffect(() => {
    setSources(fallbackSources);
  }, [fallbackSources]);

  useEffect(() => {
    setCustomDrafts(settings.custom_sources);
  }, [settings.custom_sources]);

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

  const patchCustomSources = useCallback(
    (customSources: CustomSourceEntry[]) => {
      setCustomDrafts(customSources);
      onPatch({ custom_sources: customSources });
      setSources(recognizedSourcesFromSettings({ ...settings, custom_sources: customSources }));
    },
    [settings, onPatch],
  );

  const setCustomSource = useCallback(
    (sourceId: string, patch: Partial<CustomSourceEntry>) => {
      setCustomDrafts((current) =>
        current.map((source) =>
          source.id === sourceId ? { ...source, ...patch } : source,
        ),
      );
    },
    [],
  );

  const commitCustomSources = useCallback(() => {
    patchCustomSources(customDrafts);
  }, [customDrafts, patchCustomSources]);

  const setCustomAutoCapture = useCallback(
    (sourceId: string, value: boolean) => {
      const next = customDrafts.map((source) =>
        source.id === sourceId ? { ...source, auto_prompt: value } : source,
      );
      patchCustomSources(next);
    },
    [customDrafts, patchCustomSources],
  );

  const addCustomSource = useCallback(() => {
    const id = nextCustomSourceId(customDrafts);
    patchCustomSources([
      ...customDrafts,
      {
        id,
        label: "New Source",
        title_patterns: ["New Source"],
        auto_prompt: true,
      },
    ]);
  }, [customDrafts, patchCustomSources]);

  const removeCustomSource = useCallback(
    (sourceId: string) => {
      patchCustomSources(customDrafts.filter((source) => source.id !== sourceId));
    },
    [customDrafts, patchCustomSources],
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
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => void stopSourceCapture()}
            >
              Pause capture
            </Button>
          ) : (
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={() => void startSourceCapture(detected.source_id)}
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
              <span>{source.title_patterns.join(", ")}</span>
            </div>
            <Switch
              label={`Auto capture ${source.label}`}
              checked={source.auto_prompt}
              onCheckedChange={(value) =>
                setBuiltInAutoCapture(source.id, value)
              }
            />
          </div>
        ))}
      </div>

      <div className="custom-source-list">
        {customDrafts.map((source) => (
          <div className="custom-source-row" key={source.id}>
            <input
              aria-label="Source name"
              className="ui-input"
              value={source.label}
              onChange={(event) =>
                setCustomSource(source.id, { label: event.currentTarget.value })
              }
              onBlur={commitCustomSources}
            />
            <input
              aria-label="Window title patterns"
              className="ui-input"
              value={source.title_patterns.join(", ")}
              onChange={(event) =>
                setCustomSource(source.id, {
                  title_patterns: splitTitlePatterns(event.currentTarget.value),
                })
              }
              onBlur={commitCustomSources}
            />
            <Switch
              label={`Auto capture ${source.label}`}
              checked={source.auto_prompt}
              onCheckedChange={(value) => setCustomAutoCapture(source.id, value)}
            />
            <Button
              aria-label={`Remove ${source.label}`}
              type="button"
              variant="ghost"
              size="icon"
              onClick={() => removeCustomSource(source.id)}
            >
              <Trash2 />
            </Button>
          </div>
        ))}
        <Button type="button" variant="secondary" size="sm" onClick={addCustomSource}>
          <Plus data-icon="inline-start" />
          Add Source
        </Button>
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
        title_patterns: source.title_patterns,
        auto_prompt: override ? override.auto_prompt : source.auto_prompt,
        custom: true,
      };
    }),
  ];
}

function splitTitlePatterns(value: string): string[] {
  return value
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

function nextCustomSourceId(sources: CustomSourceEntry[]): string {
  const used = new Set(sources.map((source) => source.id));
  let index = sources.length + 1;
  let id = `custom-source-${index}`;
  while (used.has(id)) {
    index += 1;
    id = `custom-source-${index}`;
  }
  return id;
}
