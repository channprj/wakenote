import { describe, expect, it } from "vitest";
import type { RecentTranscript } from "./types";
import {
  appendRecentAge,
  fileUrlFromPath,
  formatLocalTimestamp,
  groupTranscriptsByDay,
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

  it("groups transcripts by local recording day in newest-day order", () => {
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
    ];

    expect(groupTranscriptsByDay(transcripts).map((group) => group.day)).toEqual([
      "2026-05-10",
      "2026-05-09",
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
});
