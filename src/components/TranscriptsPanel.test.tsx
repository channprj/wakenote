import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { RecentTranscript } from "../lib/types";
import { TranscriptsPanel } from "./TranscriptsPanel";

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
});
