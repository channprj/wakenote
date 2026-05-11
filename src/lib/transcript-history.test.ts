import { describe, expect, it } from "vitest";
import type { QueueJob, RecentTranscript } from "./types";
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
  groupQueueJobsByDay,
  groupTranscriptsByDay,
  humanizeBasenameTime,
  humanizeDateSegment,
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
});
