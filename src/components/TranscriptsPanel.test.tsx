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
  it("shows one date page at a time with date pagination controls", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
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
        ]}
      />,
    );

    expect(markup).toContain("2026-05-10");
    expect(markup).toContain("new day transcript");
    expect(markup).not.toContain("middle day transcript");
    expect(markup).not.toContain("old day transcript");
    expect(markup).toContain('aria-label="Go to 2026-05-10 transcripts"');
    expect(markup).toContain(">2026-05-10</button>");
    expect(markup).toContain("Previous day");
    expect(markup).toContain("Next day");
    expect(markup).toContain('aria-current="page"');
    expect(markup).not.toContain("Page 1 of 3");
  });

  it("renders transcripts in chronological order within the selected date", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
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

  it("wraps the transcript timestamp in a file:// link to the .txt sidecar", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
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
      '<a href="file:///tmp/WakeNote/20260510/010203.txt" title="/tmp/WakeNote/20260510/010203.txt"><span>2026-05-10 01:02:03</span></a>',
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

  it("surfaces a disabled-reason tooltip on the Previous day button when on the latest day", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
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

    // groupTranscriptsByDay sorts newest-first, so activePage=0 shows 2026-05-10.
    // Previous day (which would decrement activePage) is disabled and carries the latest-day tooltip;
    // Next day is enabled and has no title attribute.
    expect(markup).toMatch(
      /<button[^>]*aria-label="Previous day"[^>]*disabled=""[^>]*title="Already on the latest day"[^>]*>/,
    );
    expect(markup).not.toMatch(/aria-label="Next day[^"]*"[^>]*title="/);
  });

  it("omits the disabled-reason tooltip on the enabled Next day button on the initial page", () => {
    const markup = renderToStaticMarkup(
      <TranscriptsPanel
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

    // Initial render is activePage=0 (latest day): Next day is enabled, so no title attribute.
    // Use a negative-leak assertion to confirm we don't leak the wrong tooltip on enabled buttons.
    expect(markup).not.toMatch(/aria-label="Next day[^"]*"[^>]*\stitle="/);
    expect(markup).not.toContain('title="Already on the earliest day"');
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
  // Groups are sorted newest-first, so activePage===0 means the user is on the latest day
  // and the Previous day button (which navigates to activePage-1) has nowhere to go.
  it("returns a reason when on the latest day (activePage === 0)", () => {
    expect(previousDayDisabledReason(0)).toBe("Already on the latest day");
  });

  it("returns null when there is a previous page", () => {
    expect(previousDayDisabledReason(1)).toBeNull();
    expect(previousDayDisabledReason(5)).toBeNull();
  });

  it("returns a reason for negative activePage as a defensive guard", () => {
    expect(previousDayDisabledReason(-1)).toBe("Already on the latest day");
  });
});

describe("nextDayDisabledReason", () => {
  // Groups are sorted newest-first, so activePage===length-1 means the user is on the
  // earliest day and the Next day button (which navigates to activePage+1) has nowhere to go.
  it("returns a reason when on the earliest day", () => {
    expect(nextDayDisabledReason(2, 3)).toBe("Already on the earliest day");
  });

  it("returns null when there is a next page", () => {
    expect(nextDayDisabledReason(0, 3)).toBeNull();
    expect(nextDayDisabledReason(1, 3)).toBeNull();
  });

  it("returns a reason when activePage exceeds the last index as a defensive guard", () => {
    expect(nextDayDisabledReason(5, 3)).toBe("Already on the earliest day");
  });

  it("returns a reason for a single-day list", () => {
    expect(nextDayDisabledReason(0, 1)).toBe("Already on the earliest day");
  });
});
