import { CircleAlertIcon } from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { AppPageRouter } from "./components/AppPageRouter";
import { MeetingTranscriptionPanel } from "./components/MeetingTranscriptionPanel";
import { QueuePanel } from "./components/QueuePanel";
import { ReportHistoryPanel } from "./components/ReportHistoryPanel";
import { TranscriptsPanel } from "./components/TranscriptsPanel";
import { CapturePage } from "./components/capture/CapturePage";
import { AppFrame } from "./components/shell/AppFrame";
import { PageHeader } from "./components/shell/PageHeader";
import { RecordingStatusRail } from "./components/shell/RecordingStatusRail";
import { SettingsPage } from "./components/settings/SettingsPage";
import { Alert, AlertDescription, AlertTitle } from "./components/ui/alert";
import { newestTranscriptTextEntries } from "./lib/live-transcripts";
import { useActivityAttention } from "./hooks/use-activity-attention";
import type { PrimaryRoute, SettingsSection } from "./lib/navigation";
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
  markAllActivityRead,
  openTranscriptFolder,
  requestAccessibilityPermission,
  openAccessibilityPermissionSettings,
  requestMicrophonePermission,
  openMicrophonePermissionSettings,
  requestScreenRecordingPermission,
  pressedModifierShortcut,
  resumeDictationShortcut,
  openScreenRecordingSettings,
  openDictionaryFile,
  processNextTranscription,
  reloadDictionaryFile,
  reprocessJobs,
  trashActivityJobs,
  retryJob,
  saveOpenRouterApiKey,
  saveSettingsPatch,
  setMicrophoneInputVolume,
  skipJob,
  startLiveCapture,
  stopLiveCapture,
  suspendDictationShortcut,
  deleteOpenRouterApiKey,
  saveOpenAiApiKey,
  deleteOpenAiApiKey,
  saveSonioxApiKey,
  deleteSonioxApiKey,
  verifyModel,
} from "./lib/tauri-client";
import {
  mockSnapshot,
  pollSnapshotDependencyKey,
  shouldRefreshSnapshotForTauriEvent,
  shouldPollSnapshot,
} from "./lib/app-state";
import type { AppSnapshot, AppSettings } from "./lib/types";
import { shouldHandleFrontendHideShortcut } from "./lib/window-shortcuts";

const launchAutoStartPollWindowMs = 130_000;

function preserveRecentTranscripts(
  current: AppSnapshot,
  next: AppSnapshot,
): AppSnapshot {
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

function WorkspacePage({
  slot,
  eyebrow,
  title,
  description,
  children,
}: {
  slot: string;
  eyebrow: string;
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div data-slot={`${slot}-page`} className="primary-workspace">
      <PageHeader eyebrow={eyebrow} title={title} description={description} />
      {children}
    </div>
  );
}

export default function App() {
  const [activeRoute, setActiveRoute] = useState<PrimaryRoute>("capture");
  const [activeSettingsSection, setActiveSettingsSection] =
    useState<SettingsSection>("general");
  const [snapshot, setSnapshot] = useState<AppSnapshot>(mockSnapshot());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [transcriptLog, setTranscriptLog] = useState<TranscriptEntry[]>([]);
  const activityAttention = useActivityAttention(snapshot.queue.jobs);
  const launchAutoStartPollUntilMs = useRef(
    Date.now() + launchAutoStartPollWindowMs,
  );
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
      // Footer's recent strip only needs the most recent handful; the
      // Transcripts panel now loads per-day on its own.
      const recentTranscripts = await loadRecentTranscripts();
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
      const subscriptions: Array<
        [string, (payload: unknown) => TranscriptEvent | null]
      > = [
        [
          "live-transcript-started",
          (payload) => {
            const data = payload as {
              source_key: string;
              source_label: string;
              microphone_slot?: "primary" | "secondary" | null;
              chunk_id: number;
              started_at: string;
            };
            return {
              type: "started",
              source_key: data.source_key,
              source_label: data.source_label,
              microphone_slot: data.microphone_slot,
              chunk_id: data.chunk_id,
              started_at: data.started_at,
            };
          },
        ],
        [
          "live-transcript-partial",
          (payload) => {
            const data = payload as {
              source_key: string;
              source_label: string;
              microphone_slot?: "primary" | "secondary" | null;
              chunk_id: number;
              text: string;
            };
            return { type: "partial", ...data };
          },
        ],
        [
          "live-transcript-committed",
          (payload) => {
            const data = payload as {
              source_key: string;
              source_label: string;
              microphone_slot?: "primary" | "secondary" | null;
              chunk_id: number;
              audio_path: string;
            };
            return {
              type: "committed",
              source_key: data.source_key,
              source_label: data.source_label,
              microphone_slot: data.microphone_slot,
              chunk_id: data.chunk_id,
              audio_path: data.audio_path,
            };
          },
        ],
        [
          "live-transcript-final",
          (payload) => {
            const data = payload as {
              source_key: string;
              source_label: string;
              microphone_slot?: "primary" | "secondary" | null;
              chunk_id: number | null;
              audio_path: string;
              text: string;
              recorded_at?: string;
            };
            return {
              type: "final",
              source_key: data.source_key,
              source_label: data.source_label,
              microphone_slot: data.microphone_slot,
              chunk_id: data.chunk_id,
              audio_path: data.audio_path,
              text: data.text,
              recorded_at: data.recorded_at,
            };
          },
        ],
        [
          "live-transcript-failed",
          (payload) => {
            const data = payload as {
              source_key: string;
              source_label: string;
              microphone_slot?: "primary" | "secondary" | null;
              chunk_id: number | null;
              audio_path: string;
              error: string;
              recorded_at?: string;
            };
            return {
              type: "failed",
              source_key: data.source_key,
              source_label: data.source_label,
              microphone_slot: data.microphone_slot,
              chunk_id: data.chunk_id,
              audio_path: data.audio_path,
              error: data.error,
              recorded_at: data.recorded_at,
            };
          },
        ],
        ["source-capture-started", () => null],
        ["source-capture-stopped", () => null],
        ["source-capture-error", () => null],
        ["dictionary-changed", () => null],
        ["dictionary-file-error", () => null],
        ["microphone-input-levels-changed", () => null],
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
          if (shouldRefreshSnapshotForTauriEvent(eventName)) {
            void refreshQuietly();
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
      !shouldPollSnapshot(snapshot.status, snapshot.queue, snapshot.models, {
        launchAutoStartPending,
      })
    ) {
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
      return true;
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      return false;
    } finally {
      setBusy(false);
    }
  }

  async function moveActivityJobsToTrash(ids: number[]): Promise<number[]> {
    setBusy(true);
    setError(null);
    try {
      const outcome = await trashActivityJobs(ids);
      setSnapshot((current) =>
        preserveRecentTranscripts(current, outcome.snapshot),
      );
      if (outcome.failures.length > 0) {
        const detail = outcome.failures
          .slice(0, 3)
          .map((failure) => `${failure.audio_path}: ${failure.error}`)
          .join("; ");
        setError(
          `${outcome.failures.length} selected ${outcome.failures.length === 1 ? "recording was" : "recordings were"} not moved to Trash. ${detail}`,
        );
      }
      return outcome.removed_ids;
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
      return [];
    } finally {
      setBusy(false);
    }
  }

  const themeMode = snapshot.settings.theme_mode === "light" ? "light" : "dark";

  useEffect(() => {
    if (typeof document === "undefined") {
      return;
    }
    document.documentElement.dataset.theme = themeMode;
  }, [themeMode]);

  const transcriptEntries = [
    ...transcriptEntriesFromRecent(snapshot.recent_transcripts),
    ...transcriptLog,
  ];
  const latestTranscriptText =
    newestTranscriptTextEntries(transcriptEntries).at(-1)?.text ?? "";
  const usableModelIds = new Set(
    snapshot.models
      .filter((model) =>
        ["ready", "installed", "unloaded"].includes(model.status),
      )
      .map((model) => model.id),
  );
  const canProcessTranscription =
    !snapshot.settings.pause_all &&
    snapshot.settings.transcription_enabled &&
    snapshot.queue.jobs.some(
      (job) => job.status === "pending" && usableModelIds.has(job.model_id),
    );

  function openSettings(section: SettingsSection) {
    setActiveSettingsSection(section);
    setActiveRoute("settings");
  }

  const pages: Record<PrimaryRoute, ReactNode> = {
    capture: (
      <CapturePage
        snapshot={snapshot}
        activityAttention={activityAttention.attention}
        transcriptEntries={transcriptEntries}
        busy={busy}
        onStart={() => void runAction(startLiveCapture)}
        onStop={() => void runAction(stopLiveCapture)}
        onRefresh={() => void refresh()}
        onPatch={(patch) => void patchSettings(patch)}
        onOpenAudioSettings={() => openSettings("audio")}
      />
    ),
    meetings: (
      <WorkspacePage
        slot="meetings"
        eyebrow="Long-form audio"
        title="Meetings"
        description="Import, monitor, resume, and review long meeting recordings."
      >
        <MeetingTranscriptionPanel />
      </WorkspacePage>
    ),
    transcripts: (
      <WorkspacePage
        slot="transcripts"
        eyebrow="Short captures"
        title="Transcripts"
        description="Browse daily voice clips, play audio, and create reports."
      >
        <TranscriptsPanel
          customSources={snapshot.settings.custom_sources}
          models={snapshot.models}
          selectedModelId={snapshot.settings.selected_model}
          autoPlayNext={snapshot.settings.autoplay_next_transcript}
          openrouterKeyConfigured={snapshot.openrouter_key_configured}
          onOpenReports={() => setActiveRoute("reports")}
        />
      </WorkspacePage>
    ),
    reports: (
      <WorkspacePage
        slot="reports"
        eyebrow="Generated output"
        title="Reports"
        description="Review and download LLM-generated transcript reports."
      >
        <ReportHistoryPanel
          model={snapshot.settings.openrouter_model}
          maxIterations={snapshot.settings.llm_max_iterations}
          openrouterKeyConfigured={snapshot.openrouter_key_configured}
          onOpenIntegrationSettings={() => openSettings("integrations")}
        />
      </WorkspacePage>
    ),
    activity: (
      <WorkspacePage
        slot="activity"
        eyebrow="Processing queue"
        title="Activity"
        description="Track pending, active, completed, and failed transcription jobs."
      >
        <QueuePanel
          nowMs={activityAttention.nowMs}
          queue={snapshot.queue}
          models={snapshot.models}
          selectedModelId={snapshot.settings.selected_model}
          canProcessTranscription={canProcessTranscription}
          onImportAudioFiles={() => void runAction(chooseAudioFiles)}
          onEnqueueBacklog={() =>
            void runAction(() => enqueueBacklog(snapshot.settings.save_root))
          }
          onMarkAllRead={() => void runAction(markAllActivityRead)}
          onCancelCurrent={() => void runAction(cancelCurrentTranscription)}
          onProcessNext={() => void runAction(processNextTranscription)}
          onRetry={(id) => void runAction(() => retryJob(id))}
          onSkip={(id) => void runAction(() => skipJob(id))}
          onOpenFolder={(path) =>
            void runAction(() => openTranscriptFolder(path))
          }
          onTrash={moveActivityJobsToTrash}
          onReprocess={(ids, modelId) =>
            runAction(() => reprocessJobs(ids, modelId))
          }
        />
      </WorkspacePage>
    ),
    settings: (
      <SettingsPage
        section={activeSettingsSection}
        onSectionChange={setActiveSettingsSection}
        snapshot={snapshot}
        actions={{
          onPatch: patchSettings,
          onSetMicrophoneInputVolume: (deviceId, volumePercent) =>
            void runAction(() =>
              setMicrophoneInputVolume(deviceId, volumePercent),
            ),
          onSuspendDictationShortcut: suspendDictationShortcut,
          onResumeDictationShortcut: resumeDictationShortcut,
          onPressedModifierShortcut: pressedModifierShortcut,
          onChooseSaveRoot: () => void runAction(chooseSaveRoot),
          onRevealSaveFolder: () => void runAction(revealSaveFolder),
          onChooseModelDirectory: () => void runAction(chooseModelDirectory),
          onOpenDictionaryFile: () => void runAction(openDictionaryFile),
          onReloadDictionaryFile: () => void runAction(reloadDictionaryFile),
          onRequestAccessibilityPermission: () =>
            void runAction(
              snapshot.permissions.accessibility.can_request
                ? requestAccessibilityPermission
                : openAccessibilityPermissionSettings,
            ),
          onRequestMicrophonePermission: () =>
            void runAction(
              snapshot.permissions.microphone.can_request
                ? requestMicrophonePermission
                : openMicrophonePermissionSettings,
            ),
          onRequestScreenRecordingPermission: () =>
            void runAction(
              snapshot.permissions.screen_recording.can_request
                ? requestScreenRecordingPermission
                : openScreenRecordingSettings,
            ),
          onVerifyModel: (modelId) =>
            void runAction(() => verifyModel(modelId)),
          onDownloadModel: (modelId) =>
            void runAction(() => downloadModel(modelId)),
          onCancelModelDownload: (modelId) =>
            void runAction(() => cancelModelDownload(modelId)),
          onDeleteModel: (modelId) =>
            void runAction(() => deleteModel(modelId)),
          onSaveOpenRouterApiKey: (apiKey) =>
            void runAction(() => saveOpenRouterApiKey(apiKey)),
          onDeleteOpenRouterApiKey: () =>
            void runAction(deleteOpenRouterApiKey),
          onSaveOpenAiApiKey: (apiKey) =>
            void runAction(() => saveOpenAiApiKey(apiKey)),
          onDeleteOpenAiApiKey: () => void runAction(deleteOpenAiApiKey),
          onSaveSonioxApiKey: (apiKey) =>
            void runAction(() => saveSonioxApiKey(apiKey)),
          onDeleteSonioxApiKey: () => void runAction(deleteSonioxApiKey),
        }}
      />
    ),
  };

  return (
    <AppFrame
      activeRoute={activeRoute}
      queueAttentionCount={activityAttention.attention?.count ?? 0}
      queueAttentionTone={activityAttention.attention?.tone ?? "warning"}
      onNavigate={setActiveRoute}
      theme={themeMode}
      statusRail={
        activeRoute === "capture" ? undefined : (
          <RecordingStatusRail
            liveActive={snapshot.status.live_input_active}
            latestText={latestTranscriptText}
            onReturnToCapture={() => setActiveRoute("capture")}
          />
        )
      }
    >
      {error ? (
        <Alert variant="destructive" className="mb-2.5">
          <CircleAlertIcon />
          <AlertTitle>WakeNote could not complete the last action</AlertTitle>
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}
      <AppPageRouter route={activeRoute} pages={pages} />
    </AppFrame>
  );
}
