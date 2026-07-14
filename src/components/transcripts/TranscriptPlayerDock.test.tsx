import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { RecentTranscript } from "@/lib/types";
import { TranscriptPlayerDock } from "./TranscriptPlayerDock";

const entry: RecentTranscript = {
  transcript_path: "/tmp/20260715/010203.txt",
  audio_path: "/tmp/20260715/010203.m4a",
  recorded_at: "2026-07-15T01:02:03+09:00",
  text: "길이가 긴 재생 중 전사 문장도 전체 경로와 함께 안전하게 표시됩니다.",
};

describe("TranscriptPlayerDock", () => {
  it("renders an in-flow player with playback, autoplay, path, and close controls", () => {
    const markup = renderToStaticMarkup(
      <TranscriptPlayerDock
        entry={entry}
        paused={false}
        autoPlayNext
        onClose={() => {}}
        onPausedChange={() => {}}
        onEnded={() => {}}
      />,
    );

    expect(markup).toContain('data-slot="transcript-player-dock"');
    expect(markup).toContain('aria-label="Pause recording"');
    expect(markup).toContain("Autoplay next on");
    expect(markup).toContain('title="/tmp/20260715/010203.m4a"');
    expect(markup).toContain('aria-label="Close player"');
  });

  it("renders nothing when the entry has no playable audio", () => {
    expect(
      renderToStaticMarkup(
        <TranscriptPlayerDock
          entry={{ ...entry, audio_path: null }}
          paused
          autoPlayNext={false}
          onClose={() => {}}
          onPausedChange={() => {}}
          onEnded={() => {}}
        />,
      ),
    ).toBe("");
  });
});
