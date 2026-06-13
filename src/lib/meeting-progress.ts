/**
 * Pure formatting/derivation helpers for the meeting transcription UI.
 *
 * Kept separate from the React panel so the progress math (percent, ETA,
 * clock formatting, status presentation) is unit-testable in isolation.
 */

import type { StatusTone } from "./status-summary";
import type { MeetingStatus } from "./types";

/** Overall progress as an integer 0..100, by audio time processed. */
export function progressPercent(processedMs: number, durationMs: number): number {
  if (durationMs <= 0) {
    return 0;
  }
  const pct = (processedMs / durationMs) * 100;
  return Math.max(0, Math.min(100, Math.round(pct)));
}

/** `h:mm:ss` for >= 1h, otherwise `m:ss`. Negative/NaN clamps to `0:00`. */
export function formatClock(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) {
    return "0:00";
  }
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const pad = (value: number) => value.toString().padStart(2, "0");
  if (hours > 0) {
    return `${hours}:${pad(minutes)}:${pad(seconds)}`;
  }
  return `${minutes}:${pad(seconds)}`;
}

/** Human ETA like `약 11분 20초` / `약 45초`, or `—` when unknown. */
export function formatEta(etaMs: number): string {
  if (!Number.isFinite(etaMs) || etaMs <= 0) {
    return "—";
  }
  const totalSeconds = Math.round(etaMs / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return `약 ${hours}시간 ${minutes}분`;
  }
  if (minutes > 0) {
    return `약 ${minutes}분 ${seconds}초`;
  }
  return `약 ${seconds}초`;
}

const STATUS_LABELS: Record<MeetingStatus, string> = {
  pending: "대기 중",
  processing: "전사 중",
  completed: "완료",
  failed: "실패",
  canceled: "취소됨",
};

export function meetingStatusLabel(status: MeetingStatus): string {
  return STATUS_LABELS[status] ?? status;
}

const STATUS_TONES: Record<MeetingStatus, StatusTone> = {
  pending: "neutral",
  processing: "primary",
  completed: "success",
  failed: "danger",
  canceled: "warning",
};

export function meetingStatusTone(status: MeetingStatus): StatusTone {
  return STATUS_TONES[status] ?? "neutral";
}

export function isMeetingActive(status: MeetingStatus): boolean {
  return status === "pending" || status === "processing";
}

/** A meeting can be resumed when it stopped before finishing all segments. */
export function canResumeMeeting(status: MeetingStatus): boolean {
  return status === "failed" || status === "canceled";
}
