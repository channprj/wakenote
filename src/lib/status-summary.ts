import type { AppMode, AppSnapshot, QueueSnapshot, TrayState } from "./types";

export type StatusTone = "neutral" | "success" | "warning" | "danger" | "primary";

export interface CaptureStatusPresentation {
  headline: string;
  detail: string;
  tone: StatusTone;
  modeLabel: string;
  microphone: string;
  threshold: string;
  queueSummary: string;
  levelSummary: string;
  runtimeWarning: string | null;
  warning: CaptureWarning | null;
}

export interface CaptureWarning {
  key: string;
  message: string;
  tone: "warning" | "danger";
}

// Mirrors QueuePanel queue-stats' failed-count tone signal at the App-level Queue summary card
// so the user sees a danger accent in the workspace header (not only inside the Queue section).
export function queueCardTone(queue: Pick<QueueSnapshot, "failed_count">): "danger" | undefined {
  return queue.failed_count > 0 ? "danger" : undefined;
}

export function visibleWarningForDismissedKey(
  warning: CaptureWarning | null,
  dismissedWarningKey: string | null,
): CaptureWarning | null {
  return warning?.key === dismissedWarningKey ? null : warning;
}

export function nextDismissedWarningKey(
  warning: CaptureWarning | null,
  dismissedWarningKey: string | null,
): string | null {
  return warning ? dismissedWarningKey : null;
}

const trayCopy: Record<TrayState, { headline: string; tone: StatusTone }> = {
  idle: { headline: "Ready for backlog", tone: "neutral" },
  listening: { headline: "Listening for voice", tone: "primary" },
  recording: { headline: "Recording speech", tone: "success" },
  transcribing: { headline: "Transcribing queue", tone: "warning" },
  paused: { headline: "Paused", tone: "warning" },
  error: { headline: "Needs attention", tone: "danger" },
};

const modeLabels: Record<AppMode, string> = {
  recording_and_transcription: "Recording + transcription",
  recording_only: "Recording only",
  transcription_only: "Transcription only",
  paused: "Paused",
};

export function captureStatusPresentation(snapshot: AppSnapshot): CaptureStatusPresentation {
  const { settings, status, queue } = snapshot;
  const state =
    status.tray_state === "idle" &&
    (status.mode === "recording_and_transcription" || status.mode === "recording_only")
      ? { headline: "Input stopped", tone: "neutral" as const }
      : trayCopy[status.tray_state];
  const threshold = `${settings.threshold_dbfs} dBFS`;
  const microphone = status.active_microphone || settings.selected_microphone_label;
  const modeLabel = modeLabels[status.mode];
  const currentDbfs = status.live_input_active ? status.level.current_dbfs : -120;
  const peakDbfs = status.live_input_active ? status.level.peak_dbfs : -120;

  return {
    headline: state.headline,
    detail: `${microphone} · ${status.active_model} · ${threshold}`,
    tone: state.tone,
    modeLabel,
    microphone,
    threshold,
    queueSummary: `${queue.pending_count} pending · ${queue.running_count} running · ${queue.failed_count} failed`,
    levelSummary: `${Math.round(currentDbfs)} dBFS current · ${Math.round(peakDbfs)} dBFS peak`,
    runtimeWarning: status.runtime_warning ?? null,
    warning: activeWarning(snapshot),
  };
}

function activeWarning(snapshot: AppSnapshot): CaptureWarning | null {
  const microphoneWarning = snapshot.status.microphone_warning;
  if (microphoneWarning) {
    return {
      key: `microphone:${microphoneWarning}`,
      message: microphoneWarning,
      tone: "danger",
    };
  }

  const runtimeWarning = snapshot.status.runtime_warning;
  if (runtimeWarning) {
    return {
      key: `runtime:${runtimeWarning}`,
      message: runtimeWarning,
      tone: runtimeWarning.startsWith("Live input stream error:") ? "danger" : "warning",
    };
  }

  const failedCount = snapshot.queue.failed_count;
  if (failedCount > 0) {
    return {
      key: `queue-failed:${failedCount}`,
      message: `${failedCount} transcription ${failedCount === 1 ? "job needs" : "jobs need"} attention`,
      tone: "danger",
    };
  }

  return null;
}
