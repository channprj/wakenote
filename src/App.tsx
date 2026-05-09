import {
  Activity,
  AudioWaveform,
  Brain,
  Clock3,
  Folder,
  FolderOpen,
  Info,
  ListTodo,
  Mic,
  RadioTower,
  Settings2,
  Shield,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Onboarding } from "./components/Onboarding";
import { SettingsPanel } from "./components/SettingsPanel";
import { TranscriptFooter } from "./components/TranscriptFooter";
import { Badge, Button } from "./components/ui/primitives";
import {
  reduceTranscriptLog,
  type TranscriptEntry,
  type TranscriptEvent,
} from "./lib/transcript-log";
import {
  cancelModelDownload,
  cancelCurrentOperation,
  cancelCurrentTranscription,
  chooseModelDirectory,
  chooseSaveRoot,
  revealSaveFolder,
  chooseAudioFiles,
  deleteModel,
  downloadModel,
  enqueueBacklog,
  loadSnapshot,
  processNextTranscription,
  retryJob,
  saveSettingsPatch,
  skipJob,
  startLiveCapture,
  stopLiveCapture,
  verifyModel,
} from "./lib/tauri-client";
import {
  mockSnapshot,
  pollSnapshotDependencyKey,
  shouldPollSnapshot,
} from "./lib/app-state";
import {
  captureStatusPresentation,
  nextDismissedWarningKey,
  visibleWarningForDismissedKey,
} from "./lib/status-summary";
import type { AppSnapshot, AppSettings } from "./lib/types";

const sections = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "models", label: "Models", icon: Brain },
  { id: "recording", label: "Recording", icon: Mic },
  { id: "storage", label: "Storage", icon: Folder },
  { id: "privacy", label: "Privacy", icon: Shield },
  { id: "history", label: "History", icon: Clock3 },
  { id: "advanced", label: "Advanced", icon: SlidersHorizontal },
  { id: "about", label: "About", icon: Info },
];

export default function App() {
  const [activeSection, setActiveSection] = useState("general");
  const [snapshot, setSnapshot] = useState<AppSnapshot>(mockSnapshot());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dismissedWarningKey, setDismissedWarningKey] = useState<string | null>(null);
  const [transcriptLog, setTranscriptLog] = useState<TranscriptEntry[]>([]);
  const transcriptDispatch = useRef((event: TranscriptEvent) => {
    setTranscriptLog((entries) => reduceTranscriptLog(entries, event));
  });

  async function refresh() {
    setBusy(true);
    setError(null);
    try {
      setSnapshot(await loadSnapshot());
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    const unlisteners: Array<() => void> = [];

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const dispatch = transcriptDispatch.current;
      const subscriptions: Array<[string, (payload: unknown) => TranscriptEvent | null]> = [
        ["live-transcript-started", (payload) => {
          const data = payload as { chunk_id: number; started_at: string };
          return { type: "started", chunk_id: data.chunk_id, started_at: data.started_at };
        }],
        ["live-transcript-partial", (payload) => {
          const data = payload as { chunk_id: number; text: string };
          return { type: "partial", chunk_id: data.chunk_id, text: data.text };
        }],
        ["live-transcript-committed", (payload) => {
          const data = payload as { chunk_id: number; audio_path: string };
          return {
            type: "committed",
            chunk_id: data.chunk_id,
            audio_path: data.audio_path,
          };
        }],
        ["live-transcript-final", (payload) => {
          const data = payload as {
            chunk_id: number | null;
            audio_path: string;
            text: string;
          };
          return {
            type: "final",
            chunk_id: data.chunk_id,
            audio_path: data.audio_path,
            text: data.text,
          };
        }],
        ["live-transcript-failed", (payload) => {
          const data = payload as {
            chunk_id: number | null;
            audio_path: string;
            error: string;
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
        const unlisten = await listen(eventName, (rawEvent) => {
          // eslint-disable-next-line no-console
          console.log(`[sagwan FE] received ${eventName}:`, rawEvent.payload);
          const next = parse(rawEvent.payload);
          if (next) {
            dispatch(next);
          }
        });
        if (cancelled) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      }
      // eslint-disable-next-line no-console
      console.log("[sagwan FE] live transcription listeners registered");
    })();

    return () => {
      cancelled = true;
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  async function refreshQuietly() {
    try {
      setSnapshot(await loadSnapshot());
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  const pollingDependencyKey = pollSnapshotDependencyKey(
    snapshot.status,
    snapshot.queue,
    snapshot.models,
  );

  useEffect(() => {
    if (!shouldPollSnapshot(snapshot.status, snapshot.queue, snapshot.models)) {
      return;
    }

    const timer = window.setInterval(() => {
      void refreshQuietly();
    }, 100);
    return () => window.clearInterval(timer);
  }, [pollingDependencyKey]);

  async function patchSettings(patch: Partial<AppSettings>) {
    setBusy(true);
    setError(null);
    try {
      setSnapshot(await saveSettingsPatch(patch));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  async function runAction(action: () => Promise<AppSnapshot>) {
    setBusy(true);
    setError(null);
    try {
      setSnapshot(await action());
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  const statusPresentation = captureStatusPresentation(snapshot);
  useEffect(() => {
    setDismissedWarningKey((current) => nextDismissedWarningKey(statusPresentation.warning, current));
  }, [statusPresentation.warning?.key]);
  const visibleWarning = visibleWarningForDismissedKey(statusPresentation.warning, dismissedWarningKey);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand__mark">
            <AudioWaveform />
          </div>
          <div>
            <strong>Sagwan</strong>
            <span>Local transcription</span>
          </div>
        </div>
        <nav aria-label="Settings sections">
          {sections.map((section) => {
            const Icon = section.icon;
            return (
              <button
                key={section.id}
                type="button"
                data-active={activeSection === section.id}
                onClick={() => setActiveSection(section.id)}
              >
                <Icon />
                {section.label}
              </button>
            );
          })}
        </nav>
      </aside>

      <main className="workspace">
        <header className="workspace__header">
          <div className="status-hero" data-tone={statusPresentation.tone}>
            <div className="status-hero__mark">
              <RadioTower />
            </div>
            <div className="status-hero__copy">
              <div className="status-strip">
                <Badge tone={statusPresentation.tone}>{statusPresentation.modeLabel}</Badge>
                <Badge>{snapshot.status.tray_state}</Badge>
              </div>
              <h1>{statusPresentation.headline}</h1>
              <span>{statusPresentation.detail}</span>
            </div>
          </div>

          <div className="status-cards" aria-label="Capture status summary">
            <div>
              <Activity />
              <span>Level</span>
              <strong>{statusPresentation.levelSummary}</strong>
            </div>
            <div>
              <ListTodo />
              <span>Queue</span>
              <strong>{statusPresentation.queueSummary}</strong>
            </div>
            <div data-busy={busy}>
              <AudioWaveform />
              <span>Runtime</span>
              <strong>{busy ? "Syncing snapshot" : statusPresentation.microphone}</strong>
            </div>
          </div>

          <div className="status-actions">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => void runAction(revealSaveFolder)}
              title={snapshot.settings.save_root}
            >
              <FolderOpen data-icon="inline-start" />
              Open Save Folder
            </Button>
          </div>
        </header>

        {error ? <div className="error-banner">{error}</div> : null}
        {!error && visibleWarning ? (
          <div className={`warning-banner warning-banner--${visibleWarning.tone}`}>
            <span>{visibleWarning.message}</span>
            <button
              type="button"
              aria-label="Dismiss warning"
              onClick={() => setDismissedWarningKey(visibleWarning.key)}
            >
              <X />
            </button>
          </div>
        ) : null}

        <Onboarding
          settings={snapshot.settings}
          models={snapshot.models}
          microphones={snapshot.microphones}
        />

        <SettingsPanel
          activeSection={activeSection}
          snapshot={snapshot}
          onPatch={(patch) => void patchSettings(patch)}
          onRefresh={() => void refresh()}
          onStartLiveCapture={() => void runAction(startLiveCapture)}
          onStopLiveCapture={() => void runAction(stopLiveCapture)}
          onChooseSaveRoot={() => void runAction(chooseSaveRoot)}
          onChooseModelDirectory={() => void runAction(chooseModelDirectory)}
          onRevealSaveFolder={() => void runAction(revealSaveFolder)}
          onImportAudioFiles={() => void runAction(chooseAudioFiles)}
          onEnqueueBacklog={() => void runAction(() => enqueueBacklog(snapshot.settings.save_root))}
          onCancelCurrent={() => void runAction(cancelCurrentTranscription)}
          onCancelCurrentOperation={() => void runAction(cancelCurrentOperation)}
          onProcessNextTranscription={() => void runAction(processNextTranscription)}
          onRetry={(id) => void runAction(() => retryJob(id))}
          onSkip={(id) => void runAction(() => skipJob(id))}
          onVerifyModel={(modelId) => void runAction(() => verifyModel(modelId))}
          onDownloadModel={(modelId) => void runAction(() => downloadModel(modelId))}
          onCancelModelDownload={(modelId) => void runAction(() => cancelModelDownload(modelId))}
          onDeleteModel={(modelId) => void runAction(() => deleteModel(modelId))}
        />
        <TranscriptFooter
          entries={transcriptLog}
          liveActive={snapshot.status.live_input_active}
        />
      </main>
    </div>
  );
}
