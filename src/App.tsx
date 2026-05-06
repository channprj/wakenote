import {
  AudioWaveform,
  Brain,
  Clock3,
  Folder,
  Info,
  Mic,
  Settings2,
  Shield,
  SlidersHorizontal,
} from "lucide-react";
import { useEffect, useState } from "react";
import { Onboarding } from "./components/Onboarding";
import { SettingsPanel } from "./components/SettingsPanel";
import { Badge } from "./components/ui/primitives";
import {
  cancelCurrentTranscription,
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
import type { AppSnapshot, AppSettings } from "./lib/types";
import { mockSnapshot } from "./lib/app-state";

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

  useEffect(() => {
    const active =
      snapshot.status.tray_state === "listening" ||
      snapshot.status.tray_state === "recording" ||
      snapshot.status.tray_state === "transcribing" ||
      snapshot.queue.pending_count > 0 ||
      snapshot.queue.running_count > 0;
    if (!active) {
      return;
    }

    const timer = window.setInterval(() => {
      void refreshQuietly();
    }, 100);
    return () => window.clearInterval(timer);
  }, [snapshot.status.tray_state]);

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
          <div>
            <h1>Voice Capture</h1>
            <div className="status-strip">
              <Badge tone={snapshot.status.tray_state === "paused" ? "warning" : "primary"}>
                {snapshot.status.mode}
              </Badge>
              <Badge>{snapshot.status.active_model}</Badge>
              <Badge>{snapshot.settings.threshold_dbfs} dBFS</Badge>
            </div>
          </div>
          <div className="header-meter" data-busy={busy}>
            <span />
            <strong>{busy ? "Syncing" : snapshot.status.tray_state}</strong>
          </div>
        </header>

        {error ? <div className="error-banner">{error}</div> : null}

        <Onboarding settings={snapshot.settings} models={snapshot.models} />

        <SettingsPanel
          activeSection={activeSection}
          snapshot={snapshot}
          onPatch={(patch) => void patchSettings(patch)}
          onRefresh={() => void refresh()}
          onStartLiveCapture={() => void runAction(startLiveCapture)}
          onStopLiveCapture={() => void runAction(stopLiveCapture)}
          onEnqueueBacklog={() => void runAction(() => enqueueBacklog(snapshot.settings.save_root))}
          onCancelCurrent={() => void runAction(cancelCurrentTranscription)}
          onProcessNextTranscription={() => void runAction(processNextTranscription)}
          onRetry={(id) => void runAction(() => retryJob(id))}
          onSkip={(id) => void runAction(() => skipJob(id))}
          onVerifyModel={(modelId) => void runAction(() => verifyModel(modelId))}
          onDownloadModel={(modelId) => void runAction(() => downloadModel(modelId))}
          onDeleteModel={(modelId) => void runAction(() => deleteModel(modelId))}
        />
      </main>
    </div>
  );
}
