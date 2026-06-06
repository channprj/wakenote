import {
  Activity,
  AudioWaveform,
  Brain,
  Clock3,
  Folder,
  Files,
  FileAudio,
  ListTodo,
  Mic,
  RadioTower,
  Settings2,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Onboarding } from "./components/Onboarding";
import { SettingsPanel } from "./components/SettingsPanel";
import { TranscriptFooter } from "./components/TranscriptFooter";
import { Badge } from "./components/ui/primitives";
import { humanizeTrayState } from "./lib/transcript-history";
import {
  reduceTranscriptLog,
  type TranscriptEntry,
  type TranscriptEvent,
} from "./lib/transcript-log";
import {
  cancelModelDownload,
  cancelCurrentTranscription,
  chooseModelDirectory,
  chooseSaveRoot,
  revealSaveFolder,
  chooseAudioFiles,
  deleteModel,
  downloadModel,
  enqueueBacklog,
  loadRecentTranscripts,
  loadSnapshot,
  requestMicrophonePermission,
  openMicrophonePermissionSettings,
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
  levelCardTone,
  nextDismissedWarningKey,
  queueCardTone,
  runtimeCardTone,
  trayStateBadgeTone,
  visibleWarningForDismissedKey,
} from "./lib/status-summary";
import type { AppSnapshot, AppSettings } from "./lib/types";
import { shouldHandleFrontendHideShortcut } from "./lib/window-shortcuts";
import appIcon from "./assets/wakenote-app.png";

const sections = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "models", label: "Models", icon: Brain },
  { id: "recording", label: "Recording", icon: Mic },
  { id: "storage", label: "Storage", icon: Folder },
  { id: "audio", label: "Audio", icon: FileAudio },
  { id: "transcripts", label: "Transcripts", icon: Files },
  { id: "history", label: "History", icon: Clock3 },
  { id: "advanced", label: "Advanced", icon: SlidersHorizontal },
];

const launchAutoStartPollWindowMs = 130_000;

// Mirrors MAX_RECENT_TRANSCRIPT_LIMIT in src-tauri/src/main.rs. Used when
// loading transcripts for the Transcripts tab so older dates remain visible
// for users with many archived sidecars.
const MAX_RECENT_TRANSCRIPT_LIMIT = 5_000;

function preserveRecentTranscripts(current: AppSnapshot, next: AppSnapshot): AppSnapshot {
  if (next.recent_transcripts.length > 0) {
    return next;
  }

  return {
    ...next,
    recent_transcripts: current.recent_transcripts,
  };
}

function transcriptEntriesFromRecent(
  recentTranscripts: AppSnapshot["recent_transcripts"],
): TranscriptEntry[] {
  return [...recentTranscripts].reverse().map((transcript, index) => ({
    chunk_id: -1 - index,
    status: "final",
    text: transcript.text,
    started_at: transcript.recorded_at,
    recorded_at: transcript.recorded_at,
    audio_path: transcript.audio_path,
    error: null,
  }));
}

export default function App() {
  const [activeSection, setActiveSection] = useState("general");
  const [snapshot, setSnapshot] = useState<AppSnapshot>(mockSnapshot());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dismissedWarningKey, setDismissedWarningKey] = useState<string | null>(null);
  const [transcriptLog, setTranscriptLog] = useState<TranscriptEntry[]>([]);
  const launchAutoStartPollUntilMs = useRef(Date.now() + launchAutoStartPollWindowMs);
  const transcriptDispatch = useRef((event: TranscriptEvent) => {
    setTranscriptLog((entries) => reduceTranscriptLog(entries, event));
  });

  async function refresh() {
    setBusy(true);
    setError(null);
    try {
      const next = await loadSnapshot();
      setSnapshot((current) => preserveRecentTranscripts(current, next));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  async function refreshTranscripts() {
    try {
      // Request up to the backend cap so the Transcripts tab can show every
      // archived sidecar across older dates, not just the most recent batch.
      const recentTranscripts = await loadRecentTranscripts(MAX_RECENT_TRANSCRIPT_LIMIT);
      setSnapshot((current) => ({
        ...current,
        recent_transcripts: recentTranscripts,
      }));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  useEffect(() => {
    void refresh();
    void refreshTranscripts();
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || !window.__TAURI_INTERNALS__) {
      return;
    }
    const handleHideShortcut = (event: KeyboardEvent) => {
      if (!shouldHandleFrontendHideShortcut(event, navigator.platform)) {
        return;
      }
      event.preventDefault();
      void (async () => {
        const { getCurrentWindow } = await import("@tauri-apps/api/window");
        await getCurrentWindow().hide();
      })();
    };
    window.addEventListener("keydown", handleHideShortcut);
    return () => window.removeEventListener("keydown", handleHideShortcut);
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
          console.log(`[wakenote FE] received ${eventName}:`, rawEvent.payload);
          const next = parse(rawEvent.payload);
          if (next) {
            dispatch(next);
          }
          if (eventName === "live-transcript-final") {
            void refreshTranscripts();
          }
        });
        if (cancelled) {
          unlisten();
        } else {
          unlisteners.push(unlisten);
        }
      }
      // eslint-disable-next-line no-console
      console.log("[wakenote FE] live transcription listeners registered");
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
      const next = await loadSnapshot();
      setSnapshot((current) => preserveRecentTranscripts(current, next));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    }
  }

  useEffect(() => {
    if (activeSection === "transcripts") {
      void refreshTranscripts();
    }
  }, [activeSection]);

  const launchAutoStartPending =
    snapshot.settings.start_live_input_on_launch &&
    snapshot.settings.recording_enabled &&
    !snapshot.settings.pause_all &&
    !snapshot.status.live_input_active &&
    Date.now() <= launchAutoStartPollUntilMs.current;

  const pollingDependencyKey = pollSnapshotDependencyKey(
    snapshot.status,
    snapshot.queue,
    snapshot.models,
    { launchAutoStartPending },
  );

  useEffect(() => {
    if (
      !shouldPollSnapshot(
        snapshot.status,
        snapshot.queue,
        snapshot.models,
        { launchAutoStartPending },
      )
    ) {
      return;
    }

    const timer = window.setInterval(() => {
      void refreshQuietly();
    }, 100);
    return () => window.clearInterval(timer);
  }, [pollingDependencyKey]);

  // loadSnapshot leaves recent_transcripts empty, so the 100ms snapshot poll
  // never refreshes the footer's archive list. While capture or queue work is
  // running, sync recent_transcripts on a short cadence so finals appear in
  // the footer without forcing the user to open the Transcripts panel.
  useEffect(() => {
    const shouldSyncFooter =
      snapshot.status.live_input_active || snapshot.queue.running_count > 0;
    if (!shouldSyncFooter) {
      return;
    }
    const timer = window.setInterval(() => {
      void refreshTranscripts();
    }, 2_000);
    return () => window.clearInterval(timer);
  }, [snapshot.status.live_input_active, snapshot.queue.running_count]);

  async function patchSettings(patch: Partial<AppSettings>) {
    setBusy(true);
    setError(null);
    try {
      const next = await saveSettingsPatch(patch);
      setSnapshot((current) => preserveRecentTranscripts(current, next));
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
      const next = await action();
      setSnapshot((current) => preserveRecentTranscripts(current, next));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  }

  const statusPresentation = captureStatusPresentation(snapshot);
  const themeMode = snapshot.settings.theme_mode === "light" ? "light" : "dark";

  useEffect(() => {
    if (typeof document === "undefined") {
      return;
    }
    document.documentElement.dataset.theme = themeMode;
  }, [themeMode]);

  useEffect(() => {
    setDismissedWarningKey((current) => nextDismissedWarningKey(statusPresentation.warning, current));
  }, [statusPresentation.warning?.key]);
  const visibleWarning = visibleWarningForDismissedKey(statusPresentation.warning, dismissedWarningKey);
  const footerTranscriptEntries = [
    ...transcriptEntriesFromRecent(snapshot.recent_transcripts),
    ...transcriptLog,
  ];

  return (
    <div className="app-shell" data-theme={themeMode}>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand__mark">
            <img src={appIcon} alt="" />
          </div>
          <div>
            <strong>WakeNote</strong>
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
                <Badge tone={trayStateBadgeTone(snapshot.status.tray_state)}>
                  {humanizeTrayState(snapshot.status.tray_state)}
                </Badge>
              </div>
              <h1>{statusPresentation.headline}</h1>
              <span>{statusPresentation.detail}</span>
            </div>
          </div>

          <div className="status-cards" aria-label="Capture status summary">
            <div
              data-tone={levelCardTone(
                snapshot.status.live_input_active,
                snapshot.status.level.current_dbfs,
                snapshot.settings.threshold_dbfs,
              )}
            >
              <Activity />
              <span>Level</span>
              <strong>{statusPresentation.levelSummary}</strong>
            </div>
            <div data-tone={queueCardTone(snapshot.queue, statusPresentation.queueCompletedCount)}>
              <ListTodo />
              <span>Queue</span>
              <strong>{statusPresentation.queueSummary}</strong>
            </div>
            <div
              data-busy={busy}
              data-tone={runtimeCardTone(
                snapshot.status.microphone_warning,
                snapshot.status.runtime_warning,
              )}
            >
              <AudioWaveform />
              <span>Runtime</span>
              <strong>{busy ? "Syncing snapshot" : statusPresentation.microphone}</strong>
            </div>
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
          onRevealSaveFolder={() => void runAction(revealSaveFolder)}
          onChooseModelDirectory={() => void runAction(chooseModelDirectory)}
          onRequestMicrophonePermission={() =>
            void runAction(
              snapshot.permissions.microphone.can_request
                ? requestMicrophonePermission
                : openMicrophonePermissionSettings,
            )
          }
          onImportAudioFiles={() => void runAction(chooseAudioFiles)}
          onEnqueueBacklog={() => void runAction(() => enqueueBacklog(snapshot.settings.save_root))}
          onCancelCurrent={() => void runAction(cancelCurrentTranscription)}
          onProcessNextTranscription={() => void runAction(processNextTranscription)}
          onRetry={(id) => void runAction(() => retryJob(id))}
          onSkip={(id) => void runAction(() => skipJob(id))}
          onVerifyModel={(modelId) => void runAction(() => verifyModel(modelId))}
          onDownloadModel={(modelId) => void runAction(() => downloadModel(modelId))}
          onCancelModelDownload={(modelId) => void runAction(() => cancelModelDownload(modelId))}
          onDeleteModel={(modelId) => void runAction(() => deleteModel(modelId))}
        />
        <TranscriptFooter
          entries={footerTranscriptEntries}
          liveActive={snapshot.status.live_input_active}
        />
      </main>
    </div>
  );
}
