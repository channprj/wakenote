import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { RecentTranscript } from "../lib/types";
import {
  nextDayDisabledReason,
  previousDayDisabledReason,
  TranscriptsPanel,
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
  it("shows today's date page by default with date pagination controls", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-14T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260508/010203.txt",
            audio_path: "/tmp/WakeNote/20260508/010203.m4a",
            recorded_at: "2026-05-08T01:02:03+09:00",
            text: "old day transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260509/010203.txt",
            audio_path: "/tmp/WakeNote/20260509/010203.m4a",
            recorded_at: "2026-05-09T01:02:03+09:00",
            text: "middle day transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            audio_path: "/tmp/WakeNote/20260510/010203.m4a",
            recorded_at: "2026-05-10T01:02:03+09:00",
            text: "new day transcript",
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

    expect(markup).toContain("2026-05-14");
    expect(markup).toContain("today transcript");
    expect(markup).not.toContain("new day transcript");
    expect(markup).not.toContain("middle day transcript");
    expect(markup).not.toContain("old day transcript");
    expect(markup).toContain('aria-label="Go to 2026-05-14 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-10 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-09 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-08 transcripts"');
    // Empty intermediate dates (no transcripts) must not render as pages.
    expect(markup).not.toContain('aria-label="Go to 2026-05-13 transcripts"');
    expect(markup).not.toContain('aria-label="Go to 2026-05-12 transcripts"');
    expect(markup).not.toContain('aria-label="Go to 2026-05-11 transcripts"');
    expect(markup).toContain(">2026-05-14</button>");
    expect(markup).toContain("Previous date");
    expect(markup).toContain('aria-label="Previous date"');
    expect(markup).toContain('aria-label="Next date"');
    expect(markup).toContain('aria-current="page"');
    expect(markup).not.toContain("Page 1 of 3");
  });

  it("renders date page buttons chronologically with today on the right", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-14T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260512/010203.txt",
            audio_path: "/tmp/WakeNote/20260512/010203.m4a",
            recorded_at: "2026-05-12T01:02:03+09:00",
            text: "older entry",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260514/010203.txt",
            audio_path: "/tmp/WakeNote/20260514/010203.m4a",
            recorded_at: "2026-05-14T01:02:03+09:00",
            text: "today entry",
          }),
        ]}
      />,
    );

    // 2026-05-12 must appear before (left of) 2026-05-14 in the page strip.
    expect(markup.indexOf(">2026-05-12</button>")).toBeLessThan(
      markup.indexOf(">2026-05-14</button>"),
    );
    // Today is the active page even though it's the right-most button.
    expect(markup).toMatch(
      /<button[^>]*aria-current="page"[^>]*aria-label="Go to 2026-05-14 transcripts"/,
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

  it("surfaces a disabled-reason tooltip on the Next date arrow when already on today", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-10T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260509/010203.txt",
            audio_path: "/tmp/WakeNote/20260509/010203.m4a",
            recorded_at: "2026-05-09T01:02:03+09:00",
            text: "older day",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            audio_path: "/tmp/WakeNote/20260510/010203.m4a",
            recorded_at: "2026-05-10T01:02:03+09:00",
            text: "newer day",
          }),
        ]}
      />,
    );

    // The initial page is today, so the newer-date arrow is disabled and older-date navigation is enabled.
    expect(markup).toMatch(
      /<button[^>]*aria-label="Next date"[^>]*disabled=""[^>]*title="Already on today"[^>]*>/,
    );
    expect(markup).not.toMatch(/aria-label="Previous date"[^>]*\stitle="/);
  });

  it("omits the disabled-reason tooltip on the enabled Previous date button on today's page", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
        today={new Date("2026-05-10T12:00:00+09:00")}
        transcripts={[
          transcript({
            transcript_path: "/tmp/WakeNote/20260509/010203.txt",
            audio_path: "/tmp/WakeNote/20260509/010203.m4a",
            recorded_at: "2026-05-09T01:02:03+09:00",
            text: "older day",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            audio_path: "/tmp/WakeNote/20260510/010203.m4a",
            recorded_at: "2026-05-10T01:02:03+09:00",
            text: "newer day",
          }),
        ]}
      />,
    );

    expect(markup).not.toMatch(/aria-label="Previous date"[^>]*\stitle="/);
    expect(markup).not.toContain('title="Already on the earliest transcript date"');
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

describe("previousDayDisabledReason", () => {
  // Date pages are sorted chronologically (oldest -> today), so previous-date
  // navigation moves toward earlier days = lower activePage indices.
  it("returns a reason when on the earliest transcript date", () => {
    expect(previousDayDisabledReason(0)).toBe("Already on the earliest transcript date");
  });

  it("returns null when there is an older date page", () => {
    expect(previousDayDisabledReason(1)).toBeNull();
    expect(previousDayDisabledReason(5)).toBeNull();
  });

  it("returns a reason for negative activePage as a defensive guard", () => {
    expect(previousDayDisabledReason(-1)).toBe("Already on the earliest transcript date");
  });
});

describe("nextDayDisabledReason", () => {
  // Next-date navigation moves toward today/newer days = higher activePage.
  it("returns a reason when already on today", () => {
    expect(nextDayDisabledReason(2, 3)).toBe("Already on today");
  });

  it("returns null when there is a newer date page", () => {
    expect(nextDayDisabledReason(0, 3)).toBeNull();
    expect(nextDayDisabledReason(1, 3)).toBeNull();
  });

  it("returns a reason for out-of-range activePage as a defensive guard", () => {
    expect(nextDayDisabledReason(5, 3)).toBe("Already on today");
  });

  it("returns a reason for a single-day list", () => {
    expect(nextDayDisabledReason(0, 1)).toBe("Already on today");
  });
});
