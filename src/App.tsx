import {
  Activity,
  AudioWaveform,
  Brain,
  Clock3,
  Folder,
  Info,
  ListTodo,
  Mic,
  RadioTower,
  Settings2,
  Shield,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import { FloatingOverlay } from "./components/FloatingOverlay";
import { Onboarding } from "./components/Onboarding";
import { SettingsPanel } from "./components/SettingsPanel";
import { Badge } from "./components/ui/primitives";
import {
  cancelModelDownload,
  cancelCurrentTranscription,
  chooseSaveRoot,
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
  shouldShowFloatingOverlay,
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
        {shouldShowFloatingOverlay(snapshot.settings, snapshot.status.tray_state) ? (
          <FloatingOverlay status={snapshot.status} />
        ) : null}
      </main>
    </div>
  );
}
