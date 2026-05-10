import type { QueueJob, RecentTranscript } from "./types";

export interface TranscriptDayGroup<T> {
  day: string;
  entries: T[];
}

const millisecondsPerSecond = 1_000;
const millisecondsPerMinute = 60 * millisecondsPerSecond;
const millisecondsPerDay = 24 * 60 * millisecondsPerMinute;

export function formatLocalTimestamp(value: string | Date): string {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "";
  }

  return [
    date.getFullYear(),
    pad2(date.getMonth() + 1),
    pad2(date.getDate()),
  ].join("-") + ` ${pad2(date.getHours())}:${pad2(date.getMinutes())}:${pad2(date.getSeconds())}`;
}

export function appendRecentAge(
  text: string,
  recordedAt: string,
  now: Date = new Date(),
): string {
  const recordedDate = new Date(recordedAt);
  const diffMs = now.getTime() - recordedDate.getTime();
  if (Number.isNaN(recordedDate.getTime()) || diffMs < 0 || diffMs >= millisecondsPerDay) {
    return text;
  }

  if (diffMs < millisecondsPerMinute) {
    const seconds = Math.max(1, Math.floor(diffMs / millisecondsPerSecond));
    return `${text} ${seconds} seconds ago`;
  }

  const minutes = Math.max(1, Math.floor(diffMs / millisecondsPerMinute));
  return `${text} ${minutes} minutes ago`;
}

export function groupTranscriptsByDay(
  transcripts: RecentTranscript[],
): Array<TranscriptDayGroup<RecentTranscript>> {
  return groupByDay(
    transcripts,
    (transcript) => transcriptDayFromTimestamp(transcript.recorded_at),
  )
    .map((group) => ({
      ...group,
      entries: [...group.entries].sort(compareTranscriptsChronologically),
    }))
    .sort((left, right) => compareTranscriptDaysDescending(left.day, right.day));
}

export function groupQueueJobsByDay(
  jobs: QueueJob[],
): Array<TranscriptDayGroup<QueueJob>> {
  return groupByDay(jobs, (job) => transcriptDayFromAudioPath(job.audio_path));
}

export function transcriptDayFromAudioPath(audioPath: string): string {
  const match = audioPath.match(/(?:^|\/)(\d{4})(\d{2})(\d{2})(?:\/|$)/);
  if (!match) {
    return "Imported";
  }

  return `${match[1]}-${match[2]}-${match[3]}`;
}

export function fileUrlFromPath(path: string): string {
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(path)) {
    return path;
  }

  return `file://${path.split("/").map(encodeURIComponent).join("/")}`;
}

function transcriptDayFromTimestamp(recordedAt: string): string {
  const timestamp = formatLocalTimestamp(recordedAt);
  return timestamp ? timestamp.slice(0, 10) : "Unknown";
}

function groupByDay<T>(
  entries: T[],
  dayForEntry: (entry: T) => string,
): Array<TranscriptDayGroup<T>> {
  const groups = new Map<string, T[]>();
  for (const entry of entries) {
    const day = dayForEntry(entry);
    groups.set(day, [...(groups.get(day) ?? []), entry]);
  }

  return [...groups.entries()].map(([day, groupedEntries]) => ({
    day,
    entries: groupedEntries,
  }));
}

function compareTranscriptsChronologically(
  left: RecentTranscript,
  right: RecentTranscript,
): number {
  return (
    timestampSortValue(left.recorded_at) - timestampSortValue(right.recorded_at) ||
    left.transcript_path.localeCompare(right.transcript_path)
  );
}

function compareTranscriptDaysDescending(left: string, right: string): number {
  if (left === "Unknown") {
    return 1;
  }
  if (right === "Unknown") {
    return -1;
  }
  return right.localeCompare(left);
}

function timestampSortValue(value: string): number {
  const time = new Date(value).getTime();
  return Number.isNaN(time) ? Number.MAX_SAFE_INTEGER : time;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}
