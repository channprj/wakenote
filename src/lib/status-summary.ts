import { countCompletedQueueJobs } from "./transcript-history";
import type {
  AppMode,
  AppSnapshot,
  ModelStatus,
  QueueJobStatus,
  QueueSnapshot,
  TrayState,
} from "./types";

export type StatusTone = "neutral" | "success" | "warning" | "danger" | "primary";

export interface CaptureStatusPresentation {
  headline: string;
  detail: string;
  tone: StatusTone;
  modeLabel: string;
  microphone: string;
  threshold: string;
  queueSummary: string;
  queueCompletedCount: number;
  levelSummary: string;
  runtimeWarning: string | null;
  warning: CaptureWarning | null;
}

export interface CaptureWarning {
  key: string;
  message: string;
  tone: "warning" | "danger";
}

// Mirrors QueuePanel queue-stats' per-cell tone mapping at the App-level Queue summary card so
// the workspace header signals the most attention-grabbing queue state without expanding the card
// into multiple cells. Priority follows the iter-24 "most immediate blocker first" convention:
// failed (danger, needs retry) > running (primary, in flight) > pending (warning, backlog) >
// completed (success, happy-path throughput) > none.
export function queueCardTone(
  queue: Pick<QueueSnapshot, "failed_count" | "running_count" | "pending_count">,
  completedCount = 0,
): "danger" | "primary" | "warning" | "success" | undefined {
  if (queue.failed_count > 0) return "danger";
  if (queue.running_count > 0) return "primary";
  if (queue.pending_count > 0) return "warning";
  if (completedCount > 0) return "success";
  return undefined;
}

// Mirrors the queueCardTone pattern for the App-level Level summary card: when live input is
// active and the current dBFS is at or above the trigger threshold, the gate would open (speech
// detected), so the card surfaces a primary tone to match the status-hero's "Listening" /
// "Recording" cues. Silent input or below-threshold ambient noise returns undefined.
export function levelCardTone(
  liveInputActive: boolean,
  currentDbfs: number,
  thresholdDbfs: number,
): "primary" | undefined {
  if (!liveInputActive) return undefined;
  if (currentDbfs >= thresholdDbfs) return "primary";
  return undefined;
}

// Mirrors the activeWarning() severity classification for the App-level Runtime summary card:
// a microphone fallback / interpretation failure projects danger (the capture pipeline cannot
// confidently use the pinned input), a "Live input stream error:" runtime warning is also danger
// (the stream broke mid-capture), and any other recoverable runtime warning (dropped frames, etc.)
// projects warning. The absence of either field returns undefined so a healthy runtime stays plain.
export function runtimeCardTone(
  microphoneWarning: string | null | undefined,
  runtimeWarning: string | null | undefined,
): "danger" | "warning" | undefined {
  if (microphoneWarning) return "danger";
  if (runtimeWarning) {
    return runtimeWarning.startsWith("Live input stream error:") ? "danger" : "warning";
  }
  return undefined;
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

// Reuses the trayCopy mapping that already drives the first status-strip Badge tone via
// captureStatusPresentation, but bypasses captureStatusPresentation's live_input_active override
// (which forces idle recording_*/recording_and_transcription to neutral). The second Badge always
// renders the raw tray_state literal (humanizeTrayState), so its tone should track the literal too:
// idle → neutral, listening → primary, recording → success, transcribing/paused → warning,
// error → danger. Falls back to "neutral" for unknown strings to keep the Badge contract total.
export function trayStateBadgeTone(state: TrayState | string): StatusTone {
  return (trayCopy as Record<string, { tone: StatusTone } | undefined>)[state]?.tone ?? "neutral";
}

// Canonical QueueJobStatus → Badge tone mapping for the QueuePanel per-row status Badge.
// Mirrors the iter-68 trayStateBadgeTone / iter-72 modelStatusBadgeTone pattern so all
// per-row Badge tones in the app derive from a single shared helper family. Lifecycle
// semantics: running → primary (in flight), completed → success (happy path), failed /
// cancelled → danger (needs attention), skipped → warning (acknowledged but not done),
// pending → neutral (waiting, no signal). Falls back to "neutral" for unknown strings
// to keep the Badge contract total when the backend introduces future QueueJobStatus values.
export function queueJobStatusBadgeTone(status: QueueJobStatus | string): StatusTone {
  switch (status) {
    case "running":
      return "primary";
    case "completed":
      return "success";
    case "failed":
    case "cancelled":
      return "danger";
    case "skipped":
      return "warning";
    default:
      return "neutral";
  }
}

// Aggregate-count tone palette shared by QueuePanel's queue-stats banner cells and per-day
// group-row chips. Distinct from queueJobStatusBadgeTone because it encodes per-bucket
// attention semantics ("any pending work means backlog → warning") rather than per-row
// lifecycle semantics ("a pending job is just waiting → neutral"). Mapping: failed /
// cancelled → danger (needs action), pending / skipped → warning (backlog or acknowledged
// gap), running → primary (in flight), completed → success (happy-path throughput). Falls
// back to "neutral" for unknown strings so the call site can stay total over future
// QueueJobStatus additions.
export function queueStatsCellTone(status: QueueJobStatus | string): StatusTone {
  switch (status) {
    case "failed":
    case "cancelled":
      return "danger";
    case "pending":
    case "skipped":
      return "warning";
    case "running":
      return "primary";
    case "completed":
      return "success";
    default:
      return "neutral";
  }
}

// Canonical ModelStatus → Badge tone mapping shared between ModelManager's per-model row Badge
// and SettingsPanel's Models section header Badge (the selected-model label). Mirrors PRD §3.5
// model state machine semantics: ready/installed → success (usable now), downloading/verifying/
// extracting → primary (active in-flight), error → danger (needs attention), missing/unloaded →
// neutral (no signal). Falls back to "neutral" for unknown strings to keep the Badge contract
// total when the backend introduces future ModelStatus values.
export function modelStatusBadgeTone(status: ModelStatus | string): StatusTone {
  switch (status) {
    case "ready":
    case "installed":
      return "success";
    case "downloading":
    case "verifying":
    case "extracting":
      return "primary";
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}

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
  const queueCompletedCount = countCompletedQueueJobs(queue.jobs);

  return {
    headline: state.headline,
    detail: `${microphone} · ${status.active_model} · ${threshold}`,
    tone: state.tone,
    modeLabel,
    microphone,
    threshold,
    queueSummary: `${queue.pending_count} pending · ${queue.running_count} running · ${queue.failed_count} failed · ${queueCompletedCount} completed`,
    queueCompletedCount,
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
