import type { AppMode, AppSnapshot, TrayState } from "./types";

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
  const state = trayCopy[status.tray_state];
  const threshold = `${settings.threshold_dbfs} dBFS`;
  const microphone = status.active_microphone || settings.selected_microphone_label;
  const modeLabel = modeLabels[status.mode];

  return {
    headline: state.headline,
    detail: `${microphone} · ${status.active_model} · ${threshold}`,
    tone: state.tone,
    modeLabel,
    microphone,
    threshold,
    queueSummary: `${queue.pending_count} pending · ${queue.running_count} running · ${queue.failed_count} failed`,
    levelSummary: `${Math.round(status.level.current_dbfs)} dBFS current · ${Math.round(
      status.level.peak_dbfs,
    )} dBFS peak`,
  };
}
