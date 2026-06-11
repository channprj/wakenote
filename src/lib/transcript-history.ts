import type {
  ModelStatus,
  QueueJob,
  QueueJobStatus,
  QueueSnapshot,
  RecentTranscript,
  TrayState,
} from "./types";

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

export function formatTranscriptForCopy(
  entry: RecentTranscript,
  sourceLabels: Readonly<Record<string, string>> = {},
): string {
  const timestamp = formatLocalTimestamp(entry.recorded_at) || entry.recorded_at;
  return `${timestamp} [${transcriptSourceLabel(entry, sourceLabels)}] - ${entry.text}`;
}

export function formatTranscriptsForCopy(
  entries: RecentTranscript[],
  sourceLabels: Readonly<Record<string, string>> = {},
): string {
  return entries.map((entry) => formatTranscriptForCopy(entry, sourceLabels)).join("\n");
}

export function transcriptSourceLabel(
  entry: Pick<RecentTranscript, "source" | "source_label">,
  sourceLabels: Readonly<Record<string, string>> = {},
): string {
  if ((entry.source ?? "microphone") !== "system") {
    return "Mic";
  }
  if (entry.source_label && sourceLabels[entry.source_label]) {
    return sourceLabels[entry.source_label];
  }
  return humanSourceLabel(entry.source_label);
}

function humanSourceLabel(sourceLabel?: string | null): string {
  if (!sourceLabel) return "System";
  if (sourceLabel === "meet") return "Meet";
  if (sourceLabel === "youtube") return "YouTube";
  return sourceLabel
    .split(/[-_ ]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(" ");
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
  return groupByDay(jobs, (job) => transcriptDayFromAudioPath(job.audio_path))
    .map((group) => ({
      ...group,
      entries: [...group.entries].sort(compareQueueJobsChronologically),
    }))
    .sort((left, right) => compareTranscriptDaysDescending(left.day, right.day));
}

export function countFailedQueueJobs(jobs: QueueJob[]): number {
  let count = 0;
  for (const job of jobs) {
    if (job.status === "failed") {
      count += 1;
    }
  }
  return count;
}

export function countPendingQueueJobs(jobs: QueueJob[]): number {
  let count = 0;
  for (const job of jobs) {
    if (job.status === "pending") {
      count += 1;
    }
  }
  return count;
}

export function countRunningQueueJobs(jobs: QueueJob[]): number {
  let count = 0;
  for (const job of jobs) {
    if (job.status === "running") {
      count += 1;
    }
  }
  return count;
}

export function countSkippedQueueJobs(jobs: QueueJob[]): number {
  let count = 0;
  for (const job of jobs) {
    if (job.status === "skipped") {
      count += 1;
    }
  }
  return count;
}

export function countCancelledQueueJobs(jobs: QueueJob[]): number {
  let count = 0;
  for (const job of jobs) {
    if (job.status === "cancelled") {
      count += 1;
    }
  }
  return count;
}

export function countCompletedQueueJobs(jobs: QueueJob[]): number {
  let count = 0;
  for (const job of jobs) {
    if (job.status === "completed") {
      count += 1;
    }
  }
  return count;
}

export interface QueueDayBreakdownEntry {
  status: QueueJobStatus;
  count: number;
}

// Canonical per-day chip breakdown order for the QueuePanel group-row display.
// Folds six previously inline `if (count > 0) <span data-tone=...>` ternary blocks
// into one ordered array — only statuses with count > 0 are surfaced. The order
// (pending → running → completed → failed → cancelled → skipped) is pinned by
// existing SettingsPanel render tests and matches the lifecycle stages a user
// expects to scan top-to-bottom (queued work first, then in-flight, then outcomes).
const QUEUE_DAY_BREAKDOWN_ORDER: readonly QueueJobStatus[] = [
  "pending",
  "running",
  "completed",
  "failed",
  "cancelled",
  "skipped",
];

export function queueDayBreakdown(jobs: QueueJob[]): QueueDayBreakdownEntry[] {
  const counts: Record<QueueJobStatus, number> = {
    pending: 0,
    running: 0,
    completed: 0,
    failed: 0,
    cancelled: 0,
    skipped: 0,
  };
  for (const job of jobs) {
    if (job.status in counts) {
      counts[job.status] += 1;
    }
  }
  return QUEUE_DAY_BREAKDOWN_ORDER
    .map((status) => ({ status, count: counts[status] }))
    .filter((entry) => entry.count > 0);
}

export interface QueueStatsBannerEntry {
  status: QueueJobStatus;
  label: string;
  count: number;
  title: string;
}

// Canonical queue-stats banner cell order for QueuePanel's top-level summary row.
// Deliberately different from QUEUE_DAY_BREAKDOWN_ORDER (which follows the job
// lifecycle): the banner groups attention-worthy statuses together (Pending +
// Skipped, then Running, then Failed + Cancelled, then Completed) so users can
// scan backlog/in-flight/outcomes in a single glance. Iter-81's learnings
// explicitly noted these two orderings serve different display semantics and
// must NOT share an ordered-list helper.
const QUEUE_STATS_BANNER_ORDER: readonly QueueJobStatus[] = [
  "pending",
  "skipped",
  "running",
  "failed",
  "cancelled",
  "completed",
];

export function queueStatsBanner(queue: QueueSnapshot): QueueStatsBannerEntry[] {
  // Mixed data sources are intentional: QueueSnapshot exposes pending_count /
  // running_count / failed_count as canonical top-level counters from the
  // backend, while skipped / cancelled / completed counts are derived from
  // jobs[] because no top-level counter exists for them. Preserving that mix
  // (vs. deriving all six from jobs[]) keeps the banner consistent with the
  // backend's authoritative counters even if jobs[] is truncated for display.
  const counts: Record<QueueJobStatus, number> = {
    pending: queue.pending_count,
    skipped: countSkippedQueueJobs(queue.jobs),
    running: queue.running_count,
    failed: queue.failed_count,
    cancelled: countCancelledQueueJobs(queue.jobs),
    completed: countCompletedQueueJobs(queue.jobs),
  };
  return QUEUE_STATS_BANNER_ORDER.map((status) => ({
    status,
    label: humanizeQueueJobStatus(status),
    count: counts[status],
    title: summarizeQueueJobsByDay(queue.jobs.filter((job) => job.status === status)),
  }));
}

export function summarizeQueueJobsByDay(jobs: QueueJob[]): string {
  if (jobs.length === 0) {
    return "";
  }

  return groupQueueJobsByDay(jobs)
    .map((group) => `${group.day}: ${group.entries.length}`)
    .join(" · ");
}

export function transcriptDayFromAudioPath(audioPath: string): string {
  const match = audioPath.match(/(?:^|\/)(\d{4})(\d{2})(\d{2})(?:\/|$)/);
  if (!match) {
    return "Imported";
  }

  return `${match[1]}-${match[2]}-${match[3]}`;
}

export function formatAudioPathLabel(audioPath: string): string {
  if (!audioPath) {
    return audioPath;
  }

  const segments = audioPath.split("/");
  const last = segments[segments.length - 1];
  if (!last) {
    return audioPath;
  }

  const humanizedLast = humanizeBasenameTime(last);
  const parent = segments.length >= 2 ? segments[segments.length - 2] : "";
  if (/^\d{8}$/.test(parent)) {
    return `${humanizeDateSegment(parent)}/${humanizedLast}`;
  }

  return humanizedLast;
}

export function humanizeBasenameTime(basename: string): string {
  // 010203.m4a → 01:02:03.m4a; 010203-2.wav → 01:02:03-2.wav
  return basename.replace(/^(\d{2})(\d{2})(\d{2})(?=[.\-])/, "$1:$2:$3");
}

export function humanizeDateSegment(segment: string): string {
  // 20260510 → 2026-05-10; non-YYYYMMDD inputs pass through unchanged.
  return segment.replace(/^(\d{4})(\d{2})(\d{2})$/, "$1-$2-$3");
}

export function humanizeQueueJobStatus(status: string): string {
  // pending → Pending; matches the banner cell labels' Title Case convention.
  if (status.length === 0) {
    return status;
  }
  return status.charAt(0).toUpperCase() + status.slice(1);
}

export function humanizeTrayState(state: TrayState | string): string {
  // idle → Idle; matches humanizeQueueJobStatus's Title Case convention for status badges.
  if (state.length === 0) {
    return state;
  }
  return state.charAt(0).toUpperCase() + state.slice(1);
}

export function humanizeModelStatus(status: ModelStatus | string): string {
  // ready → Ready; matches humanizeQueueJobStatus's Title Case convention for status badges.
  if (status.length === 0) {
    return status;
  }
  return status.charAt(0).toUpperCase() + status.slice(1);
}

export function queueJobSidecarPath(
  audioPath: string,
  status: QueueJobStatus,
): string | null {
  // Completed jobs write `.txt`; failed jobs write `.error.txt` (PRD §3.2).
  // Other statuses (pending/running/cancelled/skipped) have no sidecar yet.
  if (status === "completed") {
    return swapAudioExtension(audioPath, ".txt");
  }
  if (status === "failed") {
    return swapAudioExtension(audioPath, ".error.txt");
  }
  return null;
}

function swapAudioExtension(audioPath: string, replacement: string): string | null {
  if (!/\.(m4a|wav)$/i.test(audioPath)) {
    return null;
  }
  return audioPath.replace(/\.(m4a|wav)$/i, replacement);
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

function compareQueueJobsChronologically(left: QueueJob, right: QueueJob): number {
  const leftKey = queueJobTimeKey(left.audio_path);
  const rightKey = queueJobTimeKey(right.audio_path);
  if (leftKey && rightKey) {
    const diff = leftKey.localeCompare(rightKey);
    if (diff !== 0) {
      return diff;
    }
  } else if (leftKey && !rightKey) {
    return -1;
  } else if (!leftKey && rightKey) {
    return 1;
  }
  return left.id - right.id;
}

function queueJobTimeKey(audioPath: string): string {
  const segments = audioPath.split("/");
  const basename = segments[segments.length - 1] ?? "";
  const match = basename.match(/^(\d{6})/);
  return match ? match[1] : "";
}

function compareTranscriptDaysDescending(left: string, right: string): number {
  const leftIsDate = isYearMonthDayLabel(left);
  const rightIsDate = isYearMonthDayLabel(right);
  if (leftIsDate && !rightIsDate) {
    return -1;
  }
  if (!leftIsDate && rightIsDate) {
    return 1;
  }
  if (!leftIsDate && !rightIsDate) {
    return left.localeCompare(right);
  }
  return right.localeCompare(left);
}

function isYearMonthDayLabel(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value);
}

function timestampSortValue(value: string): number {
  const time = new Date(value).getTime();
  return Number.isNaN(time) ? Number.MAX_SAFE_INTEGER : time;
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}
