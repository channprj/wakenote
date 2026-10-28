import { describe, expect, it } from "vitest";
import type { QueueJob, QueueJobStatus, QueueSnapshot, RecentTranscript } from "./types";
import {
  appendRecentAge,
  countCancelledQueueJobs,
  countCompletedQueueJobs,
  countFailedQueueJobs,
  countPendingQueueJobs,
  countRunningQueueJobs,
  countSkippedQueueJobs,
  countUnreadActivityOutcomes,
  fileUrlFromPath,
  formatAudioPathLabel,
  formatLocalTimestamp,
  formatTranscriptForCopy,
  formatTranscriptsForCopy,
  formatTranscriptsForTextExport,
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
  transcriptDayFromRecordingReference,
  transcriptSourceLabel,
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
    expect(appendRecentAge("old transcript", "2026-05-09T07:59:59Z", now)).toBe(
      "old transcript",
    );
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
    expect(groups.map((group) => group.day)).toEqual([
      "2026-05-10",
      "2026-05-09",
    ]);
    expect(groups[0].entries.map((entry) => entry.text)).toEqual([
      "newer day first",
      "newer day",
    ]);
  });

  it("derives day labels and file links from stored recording paths", () => {
    expect(
      transcriptDayFromAudioPath("/tmp/WakeNote/20260510/010203.m4a"),
    ).toBe("2026-05-10");
    expect(fileUrlFromPath("/tmp/WakeNote/20260510/010203 voice.m4a")).toBe(
      "file:///tmp/WakeNote/20260510/010203%20voice.m4a",
    );
  });

  it("derives refresh days from recording timestamps before completion-time fallbacks", () => {
    expect(
      transcriptDayFromRecordingReference(
        "2026-05-09T08:00:00+09:00",
        "/tmp/WakeNote/20260615/235959.m4a",
        new Date("2026-06-15T12:00:00+09:00"),
      ),
    ).toBe("2026-05-09");
    expect(
      transcriptDayFromRecordingReference(
        undefined,
        "/tmp/WakeNote/20260509/080000.m4a",
        new Date("2026-06-15T12:00:00+09:00"),
      ),
    ).toBe("2026-05-09");
    expect(
      transcriptDayFromRecordingReference(
        "not-a-date",
        "/tmp/imported/meeting.wav",
        new Date("2026-06-15T12:00:00+09:00"),
      ),
    ).toBe("2026-06-15");
  });

  it("shortens audio paths to humanized day/time labels", () => {
    const cases: Array<[string, string]> = [
      // Save-root paths become a YYYY-MM-DD/HH:MM:SS pair.
      ["/Users/me/Documents/WakeNote/20260510/010203.m4a", "2026-05-10/01:02:03.m4a"],
      ["/Users/me/Documents/WakeNote/20260510/010203-001.wav", "2026-05-10/01:02:03-001.wav"],
      ["/Users/me/Documents/WakeNote/20260510/260510-010203.m4a", "2026-05-10/01:02:03.m4a"],
      // Falls back to the basename when the parent dir is not an 8-digit date.
      ["/tmp/imported/pending.wav", "pending.wav"],
      ["/tmp/2026/010203.m4a", "01:02:03.m4a"],
      ["standalone.m4a", "standalone.m4a"],
      // Nothing to shorten.
      ["", ""],
    ];

    for (const [path, expected] of cases) {
      expect(formatAudioPathLabel(path), path).toBe(expected);
    }
  });

  it("humanizes the HHMMSS clock in recording basenames", () => {
    const cases: Array<[string, string]> = [
      // Leading HHMMSS followed by an extension or collision suffix.
      ["010203.m4a", "01:02:03.m4a"],
      ["235959.wav", "23:59:59.wav"],
      ["010203-001.wav", "01:02:03-001.wav"],
      // Date-prefixed recording basenames.
      ["260510-010203.m4a", "01:02:03.m4a"],
      ["260510-010203-meet.wav", "01:02:03-meet.wav"],
      // No leading HHMMSS: unchanged.
      ["pending.wav", "pending.wav"],
      ["standalone.m4a", "standalone.m4a"],
      ["123456789.m4a", "123456789.m4a"],
      ["", ""],
    ];

    for (const [basename, expected] of cases) {
      expect(humanizeBasenameTime(basename), basename).toBe(expected);
    }
  });

  it("humanizes only YYYYMMDD date segments", () => {
    const cases: Array<[string, string]> = [
      ["20260510", "2026-05-10"],
      ["20260101", "2026-01-01"],
      ["99991231", "9999-12-31"],
      // Non-YYYYMMDD segments stay unchanged.
      ["2026-05-10", "2026-05-10"],
      ["Imported", "Imported"],
      ["123456789", "123456789"],
      ["", ""],
    ];

    for (const [segment, expected] of cases) {
      expect(humanizeDateSegment(segment), segment).toBe(expected);
    }
  });

  it("title-cases every status vocabulary for badges", () => {
    const cases: Array<[string, (value: string) => string, Array<[string, string]>]> = [
      // QueueJobStatus values match the banner cell labels.
      [
        "humanizeQueueJobStatus",
        humanizeQueueJobStatus,
        [
          ["pending", "Pending"],
          ["running", "Running"],
          ["completed", "Completed"],
          ["failed", "Failed"],
          ["cancelled", "Cancelled"],
          ["skipped", "Skipped"],
          ["", ""],
        ],
      ],
      [
        "humanizeTrayState",
        humanizeTrayState,
        [
          ["idle", "Idle"],
          ["listening", "Listening"],
          ["recording", "Recording"],
          ["transcribing", "Transcribing"],
          ["paused", "Paused"],
          ["error", "Error"],
          ["", ""],
        ],
      ],
      // ModelStatus values for the ModelManager badge.
      [
        "humanizeModelStatus",
        humanizeModelStatus,
        [
          ["installed", "Installed"],
          ["missing", "Missing"],
          ["downloading", "Downloading"],
          ["verifying", "Verifying"],
          ["extracting", "Extracting"],
          ["ready", "Ready"],
          ["unloaded", "Unloaded"],
          ["error", "Error"],
          ["", ""],
        ],
      ],
    ];

    for (const [name, humanize, values] of cases) {
      for (const [value, expected] of values) {
        expect(humanize(value), `${name}("${value}")`).toBe(expected);
      }
    }
  });

  it("counts only unread failed, cancelled, and skipped Activity outcomes", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "/failed.wav",
        model_id: "m",
        status: "failed",
        is_read: false,
      },
      {
        id: 2,
        audio_path: "/cancelled.wav",
        model_id: "m",
        status: "cancelled",
      },
      {
        id: 3,
        audio_path: "/skipped.wav",
        model_id: "m",
        status: "skipped",
        is_read: false,
      },
      {
        id: 4,
        audio_path: "/read.wav",
        model_id: "m",
        status: "failed",
        is_read: true,
      },
      {
        id: 5,
        audio_path: "/pending.wav",
        model_id: "m",
        status: "pending",
        is_read: false,
      },
      {
        id: 6,
        audio_path: "/completed.wav",
        model_id: "m",
        status: "completed",
      },
    ];

    expect(countUnreadActivityOutcomes(jobs)).toBe(3);
  });

  it("derives sidecar paths for completed and failed m4a/wav jobs only", () => {
    const audio = "/Users/me/Documents/WakeNote/20260510/010203.m4a";
    const cases: Array<[string, QueueJobStatus, string | null]> = [
      [audio, "completed", "/Users/me/Documents/WakeNote/20260510/010203.txt"],
      [audio, "failed", "/Users/me/Documents/WakeNote/20260510/010203.error.txt"],
      [audio, "pending", null],
      [audio, "running", null],
      [audio, "cancelled", null],
      [audio, "skipped", null],
      // Swaps both m4a and wav extensions and preserves collision suffixes.
      ["/tmp/WakeNote/20260510/010203-001.wav", "completed", "/tmp/WakeNote/20260510/010203-001.txt"],
      ["/tmp/WakeNote/20260510/010203.WAV", "failed", "/tmp/WakeNote/20260510/010203.error.txt"],
      // Unsupported or missing extensions have no sidecar.
      ["/tmp/import/notes.mp3", "completed", null],
      ["/tmp/import/recording", "failed", null],
      ["", "completed", null],
    ];

    for (const [audioPath, status, expected] of cases) {
      expect(queueJobSidecarPath(audioPath, status), `${audioPath} (${status})`).toBe(expected);
    }
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

  it("sorts date-prefixed queue jobs by their HHMMSS component", () => {
    const jobs: QueueJob[] = [
      {
        id: 20,
        audio_path: "/tmp/WakeNote/20260510/260510-183000.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
      {
        id: 21,
        audio_path: "/tmp/WakeNote/20260510/260510-091500.m4a",
        model_id: "whisper-medium",
        status: "completed",
        error: null,
      },
    ];

    expect(
      groupQueueJobsByDay(jobs)[0].entries.map((entry) => entry.id),
    ).toEqual([21, 20]);
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
      {
        id: 5,
        audio_path: "e",
        model_id: "m",
        status: "cancelled",
        error: null,
      },
      { id: 6, audio_path: "f", model_id: "m", status: "skipped", error: null },
      {
        id: 7,
        audio_path: "g",
        model_id: "m",
        status: "completed",
        error: null,
      },
    ];

    expect(countPendingQueueJobs(jobs)).toBe(2);
    expect(countPendingQueueJobs([])).toBe(0);
    expect(
      countPendingQueueJobs([
        {
          id: 8,
          audio_path: "h",
          model_id: "m",
          status: "running",
          error: null,
        },
      ]),
    ).toBe(0);
  });

  it("counts only running queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      { id: 1, audio_path: "a", model_id: "m", status: "running", error: null },
      { id: 2, audio_path: "b", model_id: "m", status: "running", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "pending", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "failed", error: null },
      {
        id: 5,
        audio_path: "e",
        model_id: "m",
        status: "cancelled",
        error: null,
      },
      { id: 6, audio_path: "f", model_id: "m", status: "skipped", error: null },
      {
        id: 7,
        audio_path: "g",
        model_id: "m",
        status: "completed",
        error: null,
      },
    ];

    expect(countRunningQueueJobs(jobs)).toBe(2);
    expect(countRunningQueueJobs([])).toBe(0);
    expect(
      countRunningQueueJobs([
        {
          id: 8,
          audio_path: "h",
          model_id: "m",
          status: "pending",
          error: null,
        },
      ]),
    ).toBe(0);
  });

  it("counts only failed queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "a",
        model_id: "m",
        status: "failed",
        error: "boom",
      },
      { id: 2, audio_path: "b", model_id: "m", status: "failed", error: null },
      {
        id: 3,
        audio_path: "c",
        model_id: "m",
        status: "cancelled",
        error: null,
      },
      { id: 4, audio_path: "d", model_id: "m", status: "skipped", error: null },
      {
        id: 5,
        audio_path: "e",
        model_id: "m",
        status: "completed",
        error: null,
      },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countFailedQueueJobs(jobs)).toBe(2);
    expect(countFailedQueueJobs([])).toBe(0);
    expect(
      countFailedQueueJobs([
        {
          id: 8,
          audio_path: "h",
          model_id: "m",
          status: "completed",
          error: null,
        },
      ]),
    ).toBe(0);
  });

  it("counts only cancelled queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "a",
        model_id: "m",
        status: "cancelled",
        error: "cancelled by user",
      },
      {
        id: 2,
        audio_path: "b",
        model_id: "m",
        status: "cancelled",
        error: null,
      },
      { id: 3, audio_path: "c", model_id: "m", status: "failed", error: null },
      { id: 4, audio_path: "d", model_id: "m", status: "skipped", error: null },
      {
        id: 5,
        audio_path: "e",
        model_id: "m",
        status: "completed",
        error: null,
      },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countCancelledQueueJobs(jobs)).toBe(2);
    expect(countCancelledQueueJobs([])).toBe(0);
    expect(
      countCancelledQueueJobs([
        {
          id: 8,
          audio_path: "h",
          model_id: "m",
          status: "completed",
          error: null,
        },
      ]),
    ).toBe(0);
  });

  it("counts only completed queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "a",
        model_id: "m",
        status: "completed",
        error: null,
      },
      {
        id: 2,
        audio_path: "b",
        model_id: "m",
        status: "completed",
        error: null,
      },
      { id: 3, audio_path: "c", model_id: "m", status: "failed", error: null },
      {
        id: 4,
        audio_path: "d",
        model_id: "m",
        status: "cancelled",
        error: null,
      },
      { id: 5, audio_path: "e", model_id: "m", status: "skipped", error: null },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countCompletedQueueJobs(jobs)).toBe(2);
    expect(countCompletedQueueJobs([])).toBe(0);
    expect(
      countCompletedQueueJobs([
        {
          id: 8,
          audio_path: "h",
          model_id: "m",
          status: "pending",
          error: null,
        },
      ]),
    ).toBe(0);
  });

  it("counts only skipped queue jobs, ignoring other terminal and in-flight statuses", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "a",
        model_id: "m",
        status: "skipped",
        error: "model missing",
      },
      { id: 2, audio_path: "b", model_id: "m", status: "skipped", error: null },
      { id: 3, audio_path: "c", model_id: "m", status: "failed", error: null },
      {
        id: 4,
        audio_path: "d",
        model_id: "m",
        status: "cancelled",
        error: null,
      },
      {
        id: 5,
        audio_path: "e",
        model_id: "m",
        status: "completed",
        error: null,
      },
      { id: 6, audio_path: "f", model_id: "m", status: "running", error: null },
      { id: 7, audio_path: "g", model_id: "m", status: "pending", error: null },
    ];

    expect(countSkippedQueueJobs(jobs)).toBe(2);
    expect(countSkippedQueueJobs([])).toBe(0);
    expect(
      countSkippedQueueJobs([
        {
          id: 8,
          audio_path: "h",
          model_id: "m",
          status: "completed",
          error: null,
        },
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

  it("preserves the canonical lifecycle and semantic issue order regardless of insertion order", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "/a",
        model_id: "m",
        status: "skipped",
        error: null,
      },
      {
        id: 2,
        audio_path: "/a",
        model_id: "m",
        status: "cancelled",
        error: null,
      },
      { id: 3, audio_path: "/a", model_id: "m", status: "failed", error: null },
      {
        id: 4,
        audio_path: "/a",
        model_id: "m",
        status: "completed",
        error: null,
      },
      {
        id: 5,
        audio_path: "/a",
        model_id: "m",
        status: "running",
        error: null,
      },
      {
        id: 6,
        audio_path: "/a",
        model_id: "m",
        status: "pending",
        error: null,
      },
    ];
    expect(queueDayBreakdown(jobs)).toEqual([
      { status: "pending", count: 1 },
      { status: "running", count: 1 },
      { status: "completed", count: 1 },
      { status: "warning", count: 2 },
      { status: "error", count: 1 },
      { status: "cancelled", count: 1 },
      { status: "skipped", count: 1 },
    ]);
  });

  it("counts duplicates within each status bucket", () => {
    const jobs: QueueJob[] = [
      {
        id: 1,
        audio_path: "/a",
        model_id: "m",
        status: "pending",
        error: null,
      },
      {
        id: 2,
        audio_path: "/a",
        model_id: "m",
        status: "pending",
        error: null,
      },
      {
        id: 3,
        audio_path: "/a",
        model_id: "m",
        status: "failed",
        error: "boom",
      },
    ];
    expect(queueDayBreakdown(jobs)).toEqual([
      { status: "pending", count: 2 },
      { status: "error", count: 1 },
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
      "Warnings",
      "Errors",
      "Cancelled",
      "Completed",
    ]);
  });

  it("sources pending and running from top-level counters while deriving semantic issues from jobs", () => {
    // The backend may truncate jobs[] for display but keeps top-level counters authoritative.
    // The banner must surface the authoritative count, not the visible-jobs[] count.
    const queue: QueueSnapshot = {
      jobs: [],
      pending_count: 7,
      running_count: 1,
      failed_count: 3,
    };
    const banner = queueStatsBanner(queue);
    const byStatus = new Map(
      banner.map((entry) => [entry.status, entry.count]),
    );
    expect(byStatus.get("pending")).toBe(7);
    expect(byStatus.get("running")).toBe(1);
    expect(byStatus.get("warning")).toBe(0);
    expect(byStatus.get("error")).toBe(0);
  });

  it("derives skipped / cancelled / completed counts from jobs[] (no top-level counter exists)", () => {
    const queue: QueueSnapshot = {
      jobs: [
        {
          id: 1,
          audio_path: "/a",
          model_id: "m",
          status: "skipped",
          error: null,
        },
        {
          id: 2,
          audio_path: "/a",
          model_id: "m",
          status: "skipped",
          error: null,
        },
        {
          id: 3,
          audio_path: "/a",
          model_id: "m",
          status: "cancelled",
          error: null,
        },
        {
          id: 4,
          audio_path: "/a",
          model_id: "m",
          status: "completed",
          error: null,
        },
        {
          id: 5,
          audio_path: "/a",
          model_id: "m",
          status: "completed",
          error: null,
        },
        {
          id: 6,
          audio_path: "/a",
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
    const byStatus = new Map(
      banner.map((entry) => [entry.status, entry.count]),
    );
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
    // Banner and day breakdown keep different scan orders while sharing semantic issue buckets.
    // The two surfaces serve different display semantics; they must NOT share an ordered-list helper.
    const queue: QueueSnapshot = {
      jobs: [
        {
          id: 1,
          audio_path: "/a",
          model_id: "m",
          status: "pending",
          error: null,
        },
        {
          id: 2,
          audio_path: "/a",
          model_id: "m",
          status: "running",
          error: null,
        },
        {
          id: 3,
          audio_path: "/a",
          model_id: "m",
          status: "completed",
          error: null,
        },
        {
          id: 4,
          audio_path: "/a",
          model_id: "m",
          status: "failed",
          error: null,
        },
        {
          id: 5,
          audio_path: "/a",
          model_id: "m",
          status: "cancelled",
          error: null,
        },
        {
          id: 6,
          audio_path: "/a",
          model_id: "m",
          status: "skipped",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 1,
      failed_count: 1,
    };
    const bannerOrder = queueStatsBanner(queue).map((entry) => entry.status);
    const lifecycleOrder = queueDayBreakdown(queue.jobs).map(
      (entry) => entry.status,
    );
    expect(bannerOrder).not.toEqual(lifecycleOrder);
    expect(bannerOrder).toEqual([
      "pending",
      "skipped",
      "running",
      "warning",
      "error",
      "cancelled",
      "completed",
    ]);
    expect(lifecycleOrder).toEqual([
      "pending",
      "running",
      "completed",
      "warning",
      "error",
      "cancelled",
      "skipped",
    ]);
  });
});

describe("formatTranscriptForCopy", () => {
  it("labels each capture source and formats it for copy", () => {
    const youtube: RecentTranscript = {
      transcript_path: "/tmp/WakeNote/20260519/155323-youtube.txt",
      audio_path: "/tmp/WakeNote/20260519/155323-youtube.m4a",
      recorded_at: "2026-05-19T15:53:23+09:00",
      text: "youtube text",
      source: "system",
      source_label: "youtube",
    };
    // Each row checks transcriptSourceLabel (`label`) and/or the
    // `YYYY-MM-DD HH:mm:SS [source] - text` copy line (`copy`).
    const cases: Array<{
      name: string;
      entry: RecentTranscript;
      sourceLabels?: Record<string, string>;
      label?: string;
      copy?: string;
    }> = [
      {
        // The physical microphone name wins when identity metadata is present.
        name: "wired microphone",
        entry: {
          transcript_path: "",
          audio_path: "",
          recorded_at: "",
          text: "",
          source: "microphone",
          source_label: "mic-primary-wired",
          device_name: "Wired",
        },
        label: "Wired",
      },
      {
        // Merged captures keep their device metadata but are labeled as merged.
        name: "merged microphone",
        entry: {
          transcript_path: "/tmp/WakeNote/20260729/162908-mic-merged.txt",
          audio_path: "/tmp/WakeNote/20260729/162908-mic-merged.m4a",
          recorded_at: "2026-07-29T16:29:08+09:00",
          text: "merged transcript",
          source: "microphone",
          source_label: "mic-merged",
          device_id: "input-1-boya-cm40+input-2-by-v",
          device_name: "BOYA CM40 + BY-V",
          microphone_slot: null,
        },
        label: "Merged Mic",
        copy: "2026-07-29 16:29:08 [Merged Mic] - merged transcript",
      },
      {
        // Priority Audio captures are labeled separately from merged captures.
        name: "priority microphone",
        entry: {
          transcript_path: "/tmp/WakeNote/20260729/162909-mic-priority.txt",
          audio_path: "/tmp/WakeNote/20260729/162909-mic-priority.m4a",
          recorded_at: "2026-07-29T16:29:09+09:00",
          text: "priority transcript",
          source: "microphone",
          source_label: "mic-priority",
          device_id: "input-1-boya-cm40+input-2-by-v",
          device_name: "BOYA CM40 + BY-V",
          microphone_slot: null,
        },
        label: "Priority Mic",
        copy: "2026-07-29 16:29:09 [Priority Mic] - priority transcript",
      },
      {
        // Dictation captures are labeled from their persisted source label.
        name: "dictation",
        entry: {
          transcript_path: "/tmp/WakeNote/20260731/031500-dictation.txt",
          audio_path: "/tmp/WakeNote/20260731/031500-dictation.m4a",
          recorded_at: "2026-07-31T03:15:00+09:00",
          text: "dictated text",
          source: "microphone",
          source_label: "dictation",
          device_id: "input-0-boya-cm40",
          device_name: "BOYA CM40",
          microphone_slot: null,
        },
        label: "Dict",
        copy: "2026-07-31 03:15:00 [Dict] - dictated text",
      },
      {
        name: "default microphone",
        entry: {
          transcript_path: "/tmp/WakeNote/20260519/155323.txt",
          audio_path: "/tmp/WakeNote/20260519/155323.m4a",
          recorded_at: "2026-05-19T15:53:23+09:00",
          text: "슬립~",
        },
        copy: "2026-05-19 15:53:23 [Mic] - 슬립~",
      },
      // System-audio source labels.
      {
        name: "youtube",
        entry: youtube,
        copy: "2026-05-19 15:53:23 [YouTube] - youtube text",
      },
      {
        name: "meet",
        entry: {
          ...youtube,
          transcript_path: "/tmp/WakeNote/20260519/155324-meet.txt",
          recorded_at: "2026-05-19T15:53:24+09:00",
          text: "meet text",
          source_label: "meet",
        },
        copy: "2026-05-19 15:53:24 [Meet] - meet text",
      },
      {
        name: "custom source with an override",
        entry: {
          ...youtube,
          transcript_path: "/tmp/WakeNote/20260519/155325-custom-source-2.txt",
          recorded_at: "2026-05-19T15:53:25+09:00",
          text: "custom text",
          source_label: "custom-source-2",
        },
        sourceLabels: { "custom-source-2": "Spotify" },
        copy: "2026-05-19 15:53:25 [Spotify] - custom text",
      },
    ];

    for (const { name, entry, sourceLabels, label, copy } of cases) {
      if (label !== undefined) {
        expect(transcriptSourceLabel(entry, sourceLabels), name).toBe(label);
      }
      if (copy !== undefined) {
        expect(formatTranscriptForCopy(entry, sourceLabels), name).toBe(copy);
      }
    }
  });
});

describe("formatTranscriptsForCopy", () => {
  it("joins trimmed entries with single newlines", () => {
    const entries: RecentTranscript[] = [
      {
        transcript_path: "/tmp/WakeNote/20260519/155323.txt",
        audio_path: "/tmp/WakeNote/20260519/155323.m4a",
        recorded_at: "2026-05-19T15:53:23+09:00",
        text: "슬립~\n\n",
      },
      {
        transcript_path: "/tmp/WakeNote/20260519/155330.txt",
        audio_path: "/tmp/WakeNote/20260519/155330.m4a",
        recorded_at: "2026-05-19T15:53:30+09:00",
        text: "\r\n여보세요?\r\n",
      },
    ];

    expect(formatTranscriptsForCopy(entries)).toBe(
      "2026-05-19 15:53:23 [Mic] - 슬립~\n2026-05-19 15:53:30 [Mic] - 여보세요?",
    );
    expect(formatTranscriptsForCopy([])).toBe("");
  });
});

describe("formatTranscriptsForTextExport", () => {
  it("exports entries newline-separated with one trailing newline and nothing for an empty day", () => {
    const entries: RecentTranscript[] = [
      {
        transcript_path: "/tmp/WakeNote/20260824/090000.txt",
        audio_path: "/tmp/WakeNote/20260824/090000.m4a",
        recorded_at: "2026-08-24T09:00:00+09:00",
        text: "First thought\n\n",
      },
      {
        transcript_path: "/tmp/WakeNote/20260824/100000-youtube.txt",
        audio_path: "/tmp/WakeNote/20260824/100000-youtube.m4a",
        recorded_at: "2026-08-24T10:00:00+09:00",
        text: "\r\nSecond thought\r\n",
        source: "system",
        source_label: "youtube",
      },
    ];

    expect(formatTranscriptsForTextExport(entries)).toBe(
      "2026-08-24 09:00:00 [Mic] - First thought\n" +
        "2026-08-24 10:00:00 [YouTube] - Second thought\n",
    );
    expect(formatTranscriptsForTextExport([])).toBe("");
  });
});
