import { describe, expect, it } from "vitest";
import type { QueueJob, QueueSnapshot, RecentTranscript } from "./types";
import {
  appendRecentAge,
  countCancelledQueueJobs,
  countCompletedQueueJobs,
  countFailedQueueJobs,
  countPendingQueueJobs,
  countRunningQueueJobs,
  countSkippedQueueJobs,
  fileUrlFromPath,
  formatAudioPathLabel,
  formatLocalTimestamp,
  formatTranscriptForCopy,
  formatTranscriptsForCopy,
  groupQueueJobsByDay,
  groupTranscriptsByDay,
  humanizeBasenameTime,
  humanizeDateSegment,
  humanizeModelStatus,
  humanizeQueueJobStatus,
  humanizeTrayState,
  queueDayBreakdown,
  queueJobSidecarPath,
  queueStatsBanner,
  summarizeQueueJobsByDay,
  transcriptDayFromAudioPath,
} from "./transcript-history";

describe("transcript history helpers", () => {
  it("formats transcript timestamps as YYYY-MM-DD HH:mm:SS", () => {
    const timestamp = new Date(2026, 4, 10, 7, 8, 9).toISOString();

    expect(formatLocalTimestamp(timestamp)).toBe("2026-05-10 07:08:09");
  });

  it("appends seconds and minutes ago only for transcripts from the last 24 hours", () => {
    const now = new Date("2026-05-10T08:00:00Z");

    expect(
      appendRecentAge("fresh transcript", "2026-05-10T07:59:18Z", now),
    ).toBe("fresh transcript 42 seconds ago");
    expect(
      appendRecentAge("recent transcript", "2026-05-10T07:17:00Z", now),
    ).toBe("recent transcript 43 minutes ago");
    expect(
      appendRecentAge("old transcript", "2026-05-09T07:59:59Z", now),
    ).toBe("old transcript");
  });

  it("groups transcripts by newest local day with oldest entries first", () => {
    const transcripts: RecentTranscript[] = [
      {
        transcript_path: "/tmp/WakeNote/20260509/230000.txt",
        audio_path: "/tmp/WakeNote/20260509/230000.m4a",
        recorded_at: "2026-05-09T23:00:00+09:00",
        text: "older day",
      },
      {
        transcript_path: "/tmp/WakeNote/20260510/010000.txt",
        audio_path: "/tmp/WakeNote/20260510/010000.m4a",
        recorded_at: "2026-05-10T01:00:00+09:00",
        text: "newer day",
      },
      {
        transcript_path: "/tmp/WakeNote/20260510/000000.txt",
        audio_path: "/tmp/WakeNote/20260510/000000.m4a",
        recorded_at: "2026-05-10T00:00:00+09:00",
        text: "newer day first",
      },
    ];

    const groups = groupTranscriptsByDay(transcripts);
    expect(groups.map((group) => group.day)).toEqual(["2026-05-10", "2026-05-09"]);
    expect(groups[0].entries.map((entry) => entry.text)).toEqual([
      "newer day first",
      "newer day",
    ]);
  });

  it("derives day labels and file links from stored recording paths", () => {
    expect(transcriptDayFromAudioPath("/tmp/WakeNote/20260510/010203.m4a")).toBe(
      "2026-05-10",
    );
    expect(fileUrlFromPath("/tmp/WakeNote/20260510/010203 voice.m4a")).toBe(
      "file:///tmp/WakeNote/20260510/010203%20voice.m4a",
    );
  });

  it("shortens save-root audio paths to a YYYY-MM-DD/HH:MM:SS pair with humanized date + time", () => {
    expect(
      formatAudioPathLabel("/Users/me/Documents/WakeNote/20260510/010203.m4a"),
    ).toBe("2026-05-10/01:02:03.m4a");
    expect(
      formatAudioPathLabel("/Users/me/Documents/WakeNote/20260510/010203-001.wav"),
    ).toBe("2026-05-10/01:02:03-001.wav");
  });

  it("falls back to the basename when the parent dir is not an 8-digit date", () => {
    expect(formatAudioPathLabel("/tmp/imported/pending.wav")).toBe("pending.wav");
    expect(formatAudioPathLabel("/tmp/2026/010203.m4a")).toBe("01:02:03.m4a");
    expect(formatAudioPathLabel("standalone.m4a")).toBe("standalone.m4a");
  });

  it("returns the input unchanged when there is nothing to shorten", () => {
    expect(formatAudioPathLabel("")).toBe("");
  });

  it("humanizes leading HHMMSS in basenames followed by an extension or collision suffix", () => {
    expect(humanizeBasenameTime("010203.m4a")).toBe("01:02:03.m4a");
    expect(humanizeBasenameTime("235959.wav")).toBe("23:59:59.wav");
    expect(humanizeBasenameTime("010203-001.wav")).toBe("01:02:03-001.wav");
  });

  it("leaves basenames without a leading HHMMSS unchanged", () => {
    expect(humanizeBasenameTime("pending.wav")).toBe("pending.wav");
    expect(humanizeBasenameTime("standalone.m4a")).toBe("standalone.m4a");
    expect(humanizeBasenameTime("123456789.m4a")).toBe("123456789.m4a");
    expect(humanizeBasenameTime("")).toBe("");
  });

  it("humanizes 8-digit YYYYMMDD date segments into YYYY-MM-DD", () => {
    expect(humanizeDateSegment("20260510")).toBe("2026-05-10");
    expect(humanizeDateSegment("20260101")).toBe("2026-01-01");
    expect(humanizeDateSegment("99991231")).toBe("9999-12-31");
  });

  it("leaves non-YYYYMMDD segments unchanged", () => {
    expect(humanizeDateSegment("2026-05-10")).toBe("2026-05-10");
    expect(humanizeDateSegment("Imported")).toBe("Imported");
    expect(humanizeDateSegment("123456789")).toBe("123456789");
    expect(humanizeDateSegment("")).toBe("");
  });

  it("title-cases each QueueJobStatus value to match the banner cell labels", () => {
    expect(humanizeQueueJobStatus("pending")).toBe("Pending");
    expect(humanizeQueueJobStatus("running")).toBe("Running");
    expect(humanizeQueueJobStatus("completed")).toBe("Completed");
    expect(humanizeQueueJobStatus("failed")).toBe("Failed");
    expect(humanizeQueueJobStatus("cancelled")).toBe("Cancelled");
    expect(humanizeQueueJobStatus("skipped")).toBe("Skipped");
    expect(humanizeQueueJobStatus("")).toBe("");
  });

  it("title-cases each TrayState value for status badges", () => {
    expect(humanizeTrayState("idle")).toBe("Idle");
    expect(humanizeTrayState("listening")).toBe("Listening");
    expect(humanizeTrayState("recording")).toBe("Recording");
    expect(humanizeTrayState("transcribing")).toBe("Transcribing");
    expect(humanizeTrayState("paused")).toBe("Paused");
    expect(humanizeTrayState("error")).toBe("Error");
    expect(humanizeTrayState("")).toBe("");
  });

  it("title-cases each ModelStatus value for the ModelManager badge", () => {
    expect(humanizeModelStatus("installed")).toBe("Installed");
    expect(humanizeModelStatus("missing")).toBe("Missing");
    expect(humanizeModelStatus("downloading")).toBe("Downloading");
    expect(humanizeModelStatus("verifying")).toBe("Verifying");
    expect(humanizeModelStatus("extracting")).toBe("Extracting");
    expect(humanizeModelStatus("ready")).toBe("Ready");
    expect(humanizeModelStatus("unloaded")).toBe("Unloaded");
    expect(humanizeModelStatus("error")).toBe("Error");
    expect(humanizeModelStatus("")).toBe("");
  });

  it("derives sidecar transcript paths only for completed and failed queue jobs", () => {
    const audio = "/Users/me/Documents/WakeNote/20260510/010203.m4a";
    expect(queueJobSidecarPath(audio, "completed")).toBe(
      "/Users/me/Documents/WakeNote/20260510/010203.txt",
    );
    expect(queueJobSidecarPath(audio, "failed")).toBe(
      "/Users/me/Documents/WakeNote/20260510/010203.error.txt",
    );
    expect(queueJobSidecarPath(audio, "pending")).toBeNull();
    expect(queueJobSidecarPath(audio, "running")).toBeNull();
    expect(queueJobSidecarPath(audio, "cancelled")).toBeNull();
    expect(queueJobSidecarPath(audio, "skipped")).toBeNull();
  });

  it("swaps both m4a and wav extensions and preserves collision suffixes", () => {
    expect(
      queueJobSidecarPath("/tmp/WakeNote/20260510/010203-001.wav", "completed"),
    ).toBe("/tmp/WakeNote/20260510/010203-001.txt");
    expect(
      queueJobSidecarPath("/tmp/WakeNote/20260510/010203.WAV", "failed"),
    ).toBe("/tmp/WakeNote/20260510/010203.error.txt");
  });

  it("returns null when the audio path lacks a supported extension", () => {
    expect(queueJobSidecarPath("/tmp/import/notes.mp3", "completed")).toBeNull();
    expect(queueJobSidecarPath("/tmp/import/recording", "failed")).toBeNull();
    expect(queueJobSidecarPath("", "completed")).toBeNull();
  });

  it("sorts queue jobs within a day chronologically by audio path HHMMSS prefix", () => {
    const jobs: QueueJob[] = [
      {
        id: 10,
        audio_path: "/tmp/WakeNote/20260510/183000.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
      {
        id: 11,
        audio_path: "/tmp/WakeNote/20260510/091500.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
      {
        id: 12,
        audio_path: "/tmp/WakeNote/20260510/091500-001.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
    ];

    const groups = groupQueueJobsByDay(jobs);

    expect(groups).toHaveLength(1);
    expect(groups[0].day).toBe("2026-05-10");
    expect(groups[0].entries.map((entry) => entry.id)).toEqual([11, 12, 10]);
  });

  it("falls back to job id when imported audio paths have no HHMMSS prefix", () => {
    const jobs: QueueJob[] = [
      {
        id: 7,
        audio_path: "/tmp/imported/alpha.wav",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
      {
        id: 3,
        audio_path: "/tmp/imported/beta.wav",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
    ];

    const groups = groupQueueJobsByDay(jobs);

    expect(groups).toHaveLength(1);
    expect(groups[0].day).toBe("Imported");
    expect(groups[0].entries.map((entry) => entry.id)).toEqual([3, 7]);
  });

  it("counts only pending queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "a", model_id: "m", status: "pending", error: null },
      { id: 2, audio_path: "b", model_id: "m", status: "pending", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "running", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "failed", error: null },
      { id: 5, audio_path: "e", model_id: "m", status: "cancelled", error: null },
      { id: 6, audio_path: "f", model_id: "m", status: "skipped", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "completed", error: null },
    ];

    expect(countPendingQueueJobs(jobs)).toBe(2);
    expect(countPendingQueueJobs([])).toBe(0);
    expect(
      countPendingQueueJobs([
        { id: 8, audio_path: "h", model_id: "m", status: "running", error: null },
      ]),
    ).toBe(0);
  });

  it("counts only running queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "a", model_id: "m", status: "running", error: null },
      { id: 2, audio_path: "b", model_id: "m", status: "running", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "pending", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "failed", error: null },
      { id: 5, audio_path: "e", model_id: "m", status: "cancelled", error: null },
      { id: 6, audio_path: "f", model_id: "m", status: "skipped", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "completed", error: null },
    ];

    expect(countRunningQueueJobs(jobs)).toBe(2);
    expect(countRunningQueueJobs([])).toBe(0);
    expect(
      countRunningQueueJobs([
        { id: 8, audio_path: "h", model_id: "m", status: "pending", error: null },
      ]),
    ).toBe(0);
  });

  it("counts only failed queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "a", model_id: "m", status: "failed", error: "boom" },
      { id: 2, audio_path: "b", model_id: "m", status: "failed", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "cancelled", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "skipped", error: null },
      { id: 5, audio_path: "e", model_id: "m", status: "completed", error: null },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countFailedQueueJobs(jobs)).toBe(2);
    expect(countFailedQueueJobs([])).toBe(0);
    expect(
      countFailedQueueJobs([
        { id: 8, audio_path: "h", model_id: "m", status: "completed", error: null },
      ]),
    ).toBe(0);
  });

  it("counts only cancelled queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "a", model_id: "m", status: "cancelled", error: "cancelled by user" },
      { id: 2, audio_path: "b", model_id: "m", status: "cancelled", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "failed", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "skipped", error: null },
      { id: 5, audio_path: "e", model_id: "m", status: "completed", error: null },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countCancelledQueueJobs(jobs)).toBe(2);
    expect(countCancelledQueueJobs([])).toBe(0);
    expect(
      countCancelledQueueJobs([
        { id: 8, audio_path: "h", model_id: "m", status: "completed", error: null },
      ]),
    ).toBe(0);
  });

  it("counts only completed queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "a", model_id: "m", status: "completed", error: null },
      { id: 2, audio_path: "b", model_id: "m", status: "completed", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "failed", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "cancelled", error: null },
      { id: 5, audio_path: "e", model_id: "m", status: "skipped", error: null },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countCompletedQueueJobs(jobs)).toBe(2);
    expect(countCompletedQueueJobs([])).toBe(0);
    expect(
      countCompletedQueueJobs([
        { id: 8, audio_path: "h", model_id: "m", status: "pending", error: null },
      ]),
    ).toBe(0);
  });

  it("counts only skipped queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "a", model_id: "m", status: "skipped", error: "model missing" },
      { id: 2, audio_path: "b", model_id: "m", status: "skipped", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "failed", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "cancelled", error: null },
      { id: 5, audio_path: "e", model_id: "m", status: "completed", error: null },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countSkippedQueueJobs(jobs)).toBe(2);
    expect(countSkippedQueueJobs([])).toBe(0);
    expect(
      countSkippedQueueJobs([
        { id: 8, audio_path: "h", model_id: "m", status: "completed", error: null },
      ]),
    ).toBe(0);
  });

  it("orders queue day groups newest first with Imported pushed below dated days", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "/tmp/imported/standalone.wav",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
      {
        id: 2,
        audio_path: "/tmp/WakeNote/20260509/230000.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
      {
        id: 3,
        audio_path: "/tmp/WakeNote/20260510/010000.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
    ];

    const groups = groupQueueJobsByDay(jobs);

    expect(groups.map((group) => group.day)).toEqual([
      "2026-05-10",
      "2026-05-09",
      "Imported",
    ]);
    expect(groups[0].entries.map((entry) => entry.id)).toEqual([3]);
    expect(groups[1].entries.map((entry) => entry.id)).toEqual([2]);
    expect(groups[2].entries.map((entry) => entry.id)).toEqual([1]);
  });

  it("summarizes queue job counts by day with descending-date ordering and Imported sink", () => {
    expect(summarizeQueueJobsByDay([])).toBe("");

    const singleDay: QueueJob[] = [
      {
        id: 1,
        audio_path: "/tmp/WakeNote/20260510/010000.m4a",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
      {
        id: 2,
        audio_path: "/tmp/WakeNote/20260510/020000.m4a",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
    ];
    expect(summarizeQueueJobsByDay(singleDay)).toBe("2026-05-10: 2");

    const mixedDays: QueueJob[] = [
      {
        id: 3,
        audio_path: "/tmp/imported/standalone.wav",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
      {
        id: 4,
        audio_path: "/tmp/WakeNote/20260509/230000.m4a",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
      {
        id: 5,
        audio_path: "/tmp/WakeNote/20260510/010000.m4a",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
      {
        id: 6,
        audio_path: "/tmp/WakeNote/20260510/020000.m4a",
        model_id: "whisper-medium",
        status: "pending",
        error: null,
      },
    ];
    expect(summarizeQueueJobsByDay(mixedDays)).toBe(
      "2026-05-10: 2 · 2026-05-09: 1 · Imported: 1",
    );
  });

  it("returns an empty queueDayBreakdown for an empty entries list", () => {
    expect(queueDayBreakdown([])).toEqual([]);
  });

  it("filters zero-count statuses out of queueDayBreakdown", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "/tmp/WakeNote/20260510/010101.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
    ];
    expect(queueDayBreakdown(jobs)).toEqual([{ status: "completed", count: 1 }]);
  });

  it("preserves the canonical pending→running→completed→failed→cancelled→skipped order regardless of insertion order", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "/a", model_id: "m", status: "skipped", error: null },
      { id: 2, audio_path: "/a", model_id: "m", status: "cancelled", error: null },
      { id: 3, audio_path: "/a", model_id: "m", status: "failed", error: null },
      { id: 4, audio_path: "/a", model_id: "m", status: "completed", error: null },
      { id: 5, audio_path: "/a", model_id: "m", status: "running", error: null },
      { id: 6, audio_path: "/a", model_id: "m", status: "pending", error: null },
    ];
    expect(queueDayBreakdown(jobs)).toEqual([
      { status: "pending", count: 1 },
      { status: "running", count: 1 },
      { status: "completed", count: 1 },
      { status: "failed", count: 1 },
      { status: "cancelled", count: 1 },
      { status: "skipped", count: 1 },
    ]);
  });

  it("counts duplicates within each status bucket", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "/a", model_id: "m", status: "pending", error: null },
      { id: 2, audio_path: "/a", model_id: "m", status: "pending", error: null },
      { id: 3, audio_path: "/a", model_id: "m", status: "failed", error: "boom" },
    ];
    expect(queueDayBreakdown(jobs)).toEqual([
      { status: "pending", count: 2 },
      { status: "failed", count: 1 },
    ]);
  });

  it("matches the count returned by the per-status count* helpers for the same entries", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "/a", model_id: "m", status: "pending", error: null },
      { id: 2, audio_path: "/a", model_id: "m", status: "pending", error: null },
      { id: 3, audio_path: "/a", model_id: "m", status: "running", error: null },
      { id: 4, audio_path: "/a", model_id: "m", status: "completed", error: null },
      { id: 5, audio_path: "/a", model_id: "m", status: "failed", error: null },
      { id: 6, audio_path: "/a", model_id: "m", status: "cancelled", error: null },
      { id: 7, audio_path: "/a", model_id: "m", status: "skipped", error: null },
    ];
    const breakdown = queueDayBreakdown(jobs);
    const byStatus = new Map(breakdown.map((entry) => [entry.status, entry.count]));
    expect(byStatus.get("pending")).toBe(countPendingQueueJobs(jobs));
    expect(byStatus.get("running")).toBe(countRunningQueueJobs(jobs));
    expect(byStatus.get("completed")).toBe(countCompletedQueueJobs(jobs));
    expect(byStatus.get("failed")).toBe(countFailedQueueJobs(jobs));
    expect(byStatus.get("cancelled")).toBe(countCancelledQueueJobs(jobs));
    expect(byStatus.get("skipped")).toBe(countSkippedQueueJobs(jobs));
  });

  it("emits queueStatsBanner cells in the canonical pending→skipped→running→failed→cancelled→completed order", () => {
    const queue: QueueSnapshot = {
      jobs: [],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    expect(queueStatsBanner(queue).map((entry) => entry.status)).toEqual([
      "pending",
      "skipped",
      "running",
      "failed",
      "cancelled",
      "completed",
    ]);
  });

  it("humanizes queueStatsBanner labels with Title Case for each banner cell", () => {
    const queue: QueueSnapshot = {
      jobs: [],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    expect(queueStatsBanner(queue).map((entry) => entry.label)).toEqual([
      "Pending",
      "Skipped",
      "Running",
      "Failed",
      "Cancelled",
      "Completed",
    ]);
  });

  it("sources pending / running / failed counts from QueueSnapshot top-level counters even when jobs[] disagrees", () => {
    // The backend may truncate jobs[] for display but keeps top-level counters authoritative.
    // The banner must surface the authoritative count, not the visible-jobs[] count.
    const queue: QueueSnapshot = {
      jobs: [],
      pending_count: 7,
      running_count: 1,
      failed_count: 3,
    };
    const banner = queueStatsBanner(queue);
    const byStatus = new Map(banner.map((entry) => [entry.status, entry.count]));
    expect(byStatus.get("pending")).toBe(7);
    expect(byStatus.get("running")).toBe(1);
    expect(byStatus.get("failed")).toBe(3);
  });

  it("derives skipped / cancelled / completed counts from jobs[] (no top-level counter exists)", () => {
    const queue: QueueSnapshot = {
      jobs: [
        { id: 1, audio_path: "/a", model_id: "m", status: "skipped", error: null },
        { id: 2, audio_path: "/a", model_id: "m", status: "skipped", error: null },
        { id: 3, audio_path: "/a", model_id: "m", status: "cancelled", error: null },
        { id: 4, audio_path: "/a", model_id: "m", status: "completed", error: null },
        { id: 5, audio_path: "/a", model_id: "m", status: "completed", error: null },
        { id: 6, audio_path: "/a", model_id: "m", status: "completed", error: null },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    const banner = queueStatsBanner(queue);
    const byStatus = new Map(banner.map((entry) => [entry.status, entry.count]));
    expect(byStatus.get("skipped")).toBe(2);
    expect(byStatus.get("cancelled")).toBe(1);
    expect(byStatus.get("completed")).toBe(3);
  });

  it("emits per-status title summaries built from summarizeQueueJobsByDay", () => {
    const queue: QueueSnapshot = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/120000.m4a",
          model_id: "m",
          status: "completed",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/130000.m4a",
          model_id: "m",
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/120000.m4a",
          model_id: "m",
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    const banner = queueStatsBanner(queue);
    const completed = banner.find((entry) => entry.status === "completed");
    expect(completed?.title).toBe("2026-05-10: 2 · 2026-05-09: 1");
    // Statuses with zero matching jobs yield an empty title.
    expect(banner.find((entry) => entry.status === "pending")?.title).toBe("");
  });

  it("pins the banner ordering as distinct from queueDayBreakdown's lifecycle order", () => {
    // Banner: pending → skipped → running → failed → cancelled → completed
    // Lifecycle (queueDayBreakdown): pending → running → completed → failed → cancelled → skipped
    // The two surfaces serve different display semantics; they must NOT share an ordered-list helper.
    const queue: QueueSnapshot = {
      jobs: [
        { id: 1, audio_path: "/a", model_id: "m", status: "pending", error: null },
        { id: 2, audio_path: "/a", model_id: "m", status: "running", error: null },
        { id: 3, audio_path: "/a", model_id: "m", status: "completed", error: null },
        { id: 4, audio_path: "/a", model_id: "m", status: "failed", error: null },
        { id: 5, audio_path: "/a", model_id: "m", status: "cancelled", error: null },
        { id: 6, audio_path: "/a", model_id: "m", status: "skipped", error: null },
      ],
      pending_count: 1,
      running_count: 1,
      failed_count: 1,
    };
    const bannerOrder = queueStatsBanner(queue).map((entry) => entry.status);
    const lifecycleOrder = queueDayBreakdown(queue.jobs).map((entry) => entry.status);
    expect(bannerOrder).not.toEqual(lifecycleOrder);
    expect(bannerOrder).toEqual([
      "pending",
      "skipped",
      "running",
      "failed",
      "cancelled",
      "completed",
    ]);
    expect(lifecycleOrder).toEqual([
      "pending",
      "running",
      "completed",
      "failed",
      "cancelled",
      "skipped",
    ]);
  });
});

describe("formatTranscriptForCopy", () => {
  it("formats a single entry as `YYYY-MM-DD HH:mm:SS - text`", () => {
    const entry: RecentTranscript = {
      transcript_path: "/tmp/WakeNote/20260519/155323.txt",
      audio_path: "/tmp/WakeNote/20260519/155323.m4a",
      recorded_at: "2026-05-19T15:53:23+09:00",
      text: "슬립~",
    };

    expect(formatTranscriptForCopy(entry)).toBe("2026-05-19 15:53:23 - 슬립~");
  });
});

describe("formatTranscriptsForCopy", () => {
  it("joins entries with newlines in the requested format", () => {
    const entries: RecentTranscript[] = [
      {
        transcript_path: "/tmp/WakeNote/20260519/155323.txt",
        audio_path: "/tmp/WakeNote/20260519/155323.m4a",
        recorded_at: "2026-05-19T15:53:23+09:00",
        text: "슬립~",
      },
      {
        transcript_path: "/tmp/WakeNote/20260519/155330.txt",
        audio_path: "/tmp/WakeNote/20260519/155330.m4a",
        recorded_at: "2026-05-19T15:53:30+09:00",
        text: "여보세요?",
      },
    ];

    expect(formatTranscriptsForCopy(entries)).toBe(
      "2026-05-19 15:53:23 - 슬립~\n2026-05-19 15:53:30 - 여보세요?",
    );
  });

  it("returns an empty string for an empty list", () => {
    expect(formatTranscriptsForCopy([])).toBe("");
  });
});
