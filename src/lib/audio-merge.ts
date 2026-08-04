import { formatLocalTimestamp } from "./transcript-history";
import type {
  AudioMergeProgress,
  AudioMergeStage,
  RecentTranscript,
} from "./types";

const supportedAudioExtension = /\.(m4a|mp3|wav)$/i;

export type AudioMergeUiState =
  | { status: "idle" }
  | {
      status: "running";
      operation_id: string;
      stage: AudioMergeStage;
      completed_inputs: number;
      total_inputs: number;
      percent: number;
      current_input_label: string | null;
    }
  | {
      status: "saved";
      operation_id: string;
      destination_path: string;
      input_count: number;
    }
  | {
      status: "error";
      operation_id: string;
      message: string;
    };

export function eligibleAudioMergeEntries(
  entries: readonly RecentTranscript[],
): RecentTranscript[] {
  const eligible = entries
    .filter(
      (entry) =>
        Boolean(entry.audio_path?.match(supportedAudioExtension)) &&
        Number.isFinite(new Date(entry.recorded_at).getTime()),
    )
    .sort(
      (left, right) =>
        new Date(left.recorded_at).getTime() -
          new Date(right.recorded_at).getTime() ||
        (left.audio_path ?? "").localeCompare(right.audio_path ?? ""),
    );
  return eligible.length >= 2 ? eligible : [];
}

export function audioMergeDefaultFileName(
  entries: readonly RecentTranscript[],
): string {
  const eligible = eligibleAudioMergeEntries(entries);
  if (eligible.length < 2) {
    return "WakeNote-merged.m4a";
  }
  const earliest = filenameTimestamp(eligible[0].recorded_at);
  const latest = filenameTimestamp(eligible[eligible.length - 1].recorded_at);
  if (!earliest || !latest) {
    return "WakeNote-merged.m4a";
  }
  return `WakeNote-merged-${earliest.date}-${earliest.time}-to-${latest.time}.m4a`;
}

export function createAudioMergeOperationId(
  now = Date.now(),
  uuid: string | null = typeof globalThis.crypto?.randomUUID === "function"
    ? globalThis.crypto.randomUUID()
    : null,
): string {
  const suffix = uuid?.replace(/[^A-Za-z0-9_-]/g, "") || "fallback";
  return `merge-${now}-${suffix}`.slice(0, 128);
}

export function nextAudioMergeProgress(
  state: AudioMergeUiState,
  event: AudioMergeProgress,
): AudioMergeUiState {
  if (state.status === "running" && event.operation_id !== state.operation_id) {
    return state;
  }
  if (event.stage === "saved" && event.destination_path) {
    return {
      status: "saved",
      operation_id: event.operation_id,
      destination_path: event.destination_path,
      input_count: Math.max(
        state.status === "running" ? state.total_inputs : 0,
        event.total_inputs,
      ),
    };
  }
  const previous = state.status === "running" ? state : null;
  return {
    status: "running",
    operation_id: event.operation_id,
    stage: event.stage,
    completed_inputs: Math.max(
      previous?.completed_inputs ?? 0,
      event.completed_inputs,
    ),
    total_inputs: Math.max(previous?.total_inputs ?? 0, event.total_inputs),
    percent: Math.max(previous?.percent ?? 0, event.percent),
    current_input_label: event.current_input_label,
  };
}

function filenameTimestamp(value: string) {
  const formatted = formatLocalTimestamp(value);
  const match = formatted.match(
    /^(\d{4})-(\d{2})-(\d{2}) (\d{2}):(\d{2}):(\d{2})$/,
  );
  if (!match) return null;
  return {
    date: `${match[1]}${match[2]}${match[3]}`,
    time: `${match[4]}${match[5]}${match[6]}`,
  };
}
