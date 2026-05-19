import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { RecentTranscript } from "../lib/types";
import {
  TranscriptsPanel,
  addDays,
  nextWeekDisabledReason,
  previousWeekDisabledReason,
  weekStartFor,
} from "./TranscriptsPanel";

function transcript(overrides: Partial<RecentTranscript>): RecentTranscript {
  return {
    transcript_path: "/tmp/WakeNote/20260510/010203.txt",
    audio_path: "/tmp/WakeNote/20260510/010203.m4a",
    recorded_at: "2026-05-10T01:02:03+09:00",
    text: "Daily transcript text",
    ...overrides,
  };
}

describe("TranscriptsPanel", () => {
  it("shows today's date page by default with weekly calendar pagination", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-14T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260513/010203.txt",
            audio_path: "/tmp/WakeNote/20260513/010203.m4a",
            recorded_at: "2026-05-13T01:02:03+09:00",
            text: "yesterday transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260514/010203.txt",
            audio_path: "/tmp/WakeNote/20260514/010203.m4a",
            recorded_at: "2026-05-14T01:02:03+09:00",
            text: "today transcript",
          }),
        ]}
      />,
    );

    // 2026-05-14 is a Thursday; the week (Sun..Sat) spans 2026-05-10..2026-05-16.
    expect(markup).toContain("2026-05-14");
    expect(markup).toContain("today transcript");
    expect(markup).not.toContain("yesterday transcript");
    expect(markup).toContain('aria-label="Go to 2026-05-10 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-13 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-14 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-15 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-16 transcripts"');
    expect(markup).toContain('aria-label="Previous week"');
    expect(markup).toContain('aria-label="Next week"');
    expect(markup).toContain('aria-current="page"');
    expect(markup).toContain("transcript-pagination--calendar");
    expect(markup).toContain("transcript-pagination__week");
  });

  it("renders week-day cells in 일-월-화-수-목-금-토 order with weekend tone hooks", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-14T12:00:00+09:00")}
        transcripts={[]}
      />,
    );

    const labels = ["일", "월", "화", "수", "목", "금", "토"];
    let cursor = -1;
    for (const label of labels) {
      const next = markup.indexOf(`>${label}<`, cursor + 1);
      expect(next, `expected ${label} to appear after previous label`).toBeGreaterThan(cursor);
      cursor = next;
    }
    // Sundays get data-day-of-week="0" and Saturdays get "6" so CSS can color them red/blue.
    expect(markup).toContain('data-day-of-week="0"');
    expect(markup).toContain('data-day-of-week="6"');
  });

  it("disables future days within the current week and the Next-week arrow", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-14T12:00:00+09:00")}
        transcripts={[]}
      />,
    );

    // 2026-05-15 (Fri) and 2026-05-16 (Sat) are after today and must be disabled.
    expect(markup).toMatch(
      /<button[^>]*aria-label="Go to 2026-05-15 transcripts"[^>]*disabled=""/,
    );
    expect(markup).toMatch(
      /<button[^>]*aria-label="Go to 2026-05-16 transcripts"[^>]*disabled=""/,
    );
    // Next-week arrow is disabled when on the current week.
    expect(markup).toMatch(
      /<button[^>]*aria-label="Next week"[^>]*disabled=""[^>]*title="Already on this week"/,
    );
  });

  it("disables the Previous-week arrow on the earliest week with transcripts", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-14T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260513/010203.txt",
            audio_path: "/tmp/WakeNote/20260513/010203.m4a",
            recorded_at: "2026-05-13T01:02:03+09:00",
            text: "yesterday transcript",
          }),
        ]}
      />,
    );

    // The earliest entry is in the same week as today, so Previous-week is disabled.
    expect(markup).toMatch(
      /<button[^>]*aria-label="Previous week"[^>]*disabled=""[^>]*title="Already on the earliest week"/,
    );
  });

  it("starts on today's empty date instead of jumping to the newest saved transcript", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-14T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            audio_path: "/tmp/WakeNote/20260510/010203.m4a",
            recorded_at: "2026-05-10T01:02:03+09:00",
            text: "older saved transcript",
          }),
        ]}
      />,
    );

    expect(markup).toContain("2026-05-14");
    expect(markup).toContain("No transcripts for this day");
    expect(markup).not.toContain("older saved transcript");
    expect(markup).toContain('aria-label="Go to 2026-05-10 transcripts"');
  });

  it("renders transcripts in chronological order within the selected date", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-10T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/180000.txt",
            audio_path: "/tmp/WakeNote/20260510/180000.m4a",
            recorded_at: "2026-05-10T18:00:00+09:00",
            text: "evening transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/090000.txt",
            audio_path: "/tmp/WakeNote/20260510/090000.m4a",
            recorded_at: "2026-05-10T09:00:00+09:00",
            text: "morning transcript",
          }),
        ]}
      />,
    );

    expect(markup.indexOf("morning transcript")).toBeLessThan(
      markup.indexOf("evening transcript"),
    );
  });

  it("renders a right-side play button for each transcript with a recording", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-10T12:00:00+09:00")}
        transcripts={[
          transcript({
            text: "playable transcript",
          }),
        ]}
      />,
    );

    expect(markup).toContain("playable transcript");
    expect(markup).toContain('aria-label="Play recording from 2026-05-10 01:02:03"');
    expect(markup).toContain("transcript-entry__play");
  });

  it("renders transcript rows as a condensed list without a separate recording link", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-10T12:00:00+09:00")}
        transcripts={[
          transcript({
            text: "compact transcript",
          }),
        ]}
      />,
    );

    expect(markup).toContain("transcript-entry-list transcript-entry-list--condensed");
    expect(markup).toContain("transcript-entry transcript-entry--condensed");
    expect(markup).not.toContain("Open recording file");
    expect(markup).not.toContain("transcript-entry__recording");
    expect(markup).not.toContain(">Recording</a>");
    expect(markup).not.toContain("Daily transcript");
  });

  it("renders the timestamp and transcript text inline on the same row", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-10T12:00:00+09:00")}
        transcripts={[
          transcript({
            text: "inline transcript",
          }),
        ]}
      />,
    );

    const timestampIndex = markup.indexOf("2026-05-10 01:02:03");
    const textIndex = markup.indexOf("inline transcript");
    expect(timestampIndex).toBeGreaterThan(-1);
    expect(textIndex).toBeGreaterThan(timestampIndex);
    expect(markup).toContain("transcript-entry__timestamp");
    expect(markup).toContain("transcript-entry__text");
    expect(markup).not.toContain("transcript-entry__meta");
    expect(markup).not.toContain("transcript-entry__body");
    expect(markup).not.toContain("transcript-entry__actions");
  });

  it("wraps the transcript timestamp in an inline file:// link to the .txt sidecar", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-10T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            audio_path: "/tmp/WakeNote/20260510/010203.m4a",
            recorded_at: "2026-05-10T01:02:03+09:00",
            text: "clickable transcript",
          }),
        ]}
      />,
    );

    expect(markup).toContain(
      '<a class="transcript-entry__timestamp" href="file:///tmp/WakeNote/20260510/010203.txt" title="/tmp/WakeNote/20260510/010203.txt"><span>2026-05-10 01:02:03</span></a>',
    );
  });

  it("shows a bottom sheet player while a transcript is selected for playback", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        initialPlayingTranscriptPath="/tmp/WakeNote/20260510/010203.txt"
        transcripts={[
          transcript({
            text: "currently playing transcript",
          }),
        ]}
      />,
    );

    expect(markup).toContain("transcript-player-sheet");
    expect(markup).toContain("currently playing transcript");
    expect(markup).toContain('src="file:///tmp/WakeNote/20260510/010203.m4a"');
    expect(markup).toContain("Now playing");
  });

  it("renders a 전체 복사 button when the active day has transcripts", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-19T18:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260519/155323.txt",
            audio_path: "/tmp/WakeNote/20260519/155323.m4a",
            recorded_at: "2026-05-19T15:53:23+09:00",
            text: "슬립~",
          }),
        ]}
      />,
    );

    expect(markup).toContain("transcript-day__actions");
    expect(markup).toContain('aria-label="해당 일자의 모든 트랜스크립트 복사"');
    expect(markup).toContain("전체 복사");
    expect(markup).not.toContain("선택 복사");
  });

  it("hides the copy action when the active day has no transcripts", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-19T18:00:00+09:00")}
        transcripts={[]}
      />,
    );

    expect(markup).not.toContain("transcript-day__actions");
    expect(markup).not.toContain("전체 복사");
  });

  it("wraps the player sheet header strong in a file:// link to the recording", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        initialPlayingTranscriptPath="/tmp/WakeNote/20260510/010203.txt"
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            audio_path: "/tmp/WakeNote/20260510/010203.m4a",
            recorded_at: "2026-05-10T01:02:03+09:00",
            text: "clickable header transcript",
          }),
        ]}
      />,
    );

    expect(markup).toContain(
      '<a href="file:///tmp/WakeNote/20260510/010203.m4a" title="/tmp/WakeNote/20260510/010203.m4a"><strong>2026-05-10 01:02:03</strong></a>',
    );
  });
});

describe("weekStartFor", () => {
  it("returns the Sunday of the week containing the given date", () => {
    // 2026-05-14 is a Thursday → Sunday of that week is 2026-05-10.
    expect(weekStartFor("2026-05-14")).toBe("2026-05-10");
    // 2026-05-10 is already a Sunday.
    expect(weekStartFor("2026-05-10")).toBe("2026-05-10");
    // 2026-05-16 is a Saturday.
    expect(weekStartFor("2026-05-16")).toBe("2026-05-10");
  });
});

describe("addDays", () => {
  it("shifts a YYYY-MM-DD day by the requested number of days, crossing month boundaries", () => {
    expect(addDays("2026-05-10", 7)).toBe("2026-05-17");
    expect(addDays("2026-05-10", -7)).toBe("2026-05-03");
    expect(addDays("2026-05-31", 1)).toBe("2026-06-01");
  });
});

describe("previousWeekDisabledReason", () => {
  it("returns a reason when the viewed week is at or before the earliest week with transcripts", () => {
    expect(previousWeekDisabledReason("2026-05-10", "2026-05-13")).toBe(
      "Already on the earliest week",
    );
    expect(previousWeekDisabledReason("2026-05-03", "2026-05-13")).toBe(
      "Already on the earliest week",
    );
  });

  it("returns null when there is an older week with transcripts", () => {
    expect(previousWeekDisabledReason("2026-05-10", "2026-05-01")).toBeNull();
  });
});

describe("nextWeekDisabledReason", () => {
  it("returns a reason when the viewed week is at or after today's week", () => {
    expect(nextWeekDisabledReason("2026-05-10", "2026-05-14")).toBe("Already on this week");
    expect(nextWeekDisabledReason("2026-05-17", "2026-05-14")).toBe("Already on this week");
  });

  it("returns null when there is a newer week to navigate to", () => {
    expect(nextWeekDisabledReason("2026-05-03", "2026-05-14")).toBeNull();
  });
});
