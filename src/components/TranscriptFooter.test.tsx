import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TranscriptEntry } from "../lib/transcript-log";
import { TranscriptFooter, scrollTranscriptListToBottom } from "./TranscriptFooter";

function entry(overrides: Partial<TranscriptEntry>): TranscriptEntry {
  return {
    chunk_id: 1,
    status: "partial",
    text: "",
    started_at: "2026-05-09T08:00:00Z",
    audio_path: null,
    error: null,
    ...overrides,
  };
}

describe("TranscriptFooter", () => {
  it("shows live partial text instead of the waiting copy", () => {
    const markup = renderToStaticMarkup(
      <TranscriptFooter
        liveActive
        entries={[entry({ text: "오늘 회의에서 액션 아이템을 정리합니다." })]}
      />,
    );

    expect(markup).toContain("오늘 회의에서 액션 아이템을 정리합니다.");
    expect(markup).not.toContain("Waiting for the first speech segment");
  });

  it("renders the newest transcript segment at the bottom", () => {
    const markup = renderToStaticMarkup(
      <TranscriptFooter
        liveActive
        entries={[
          entry({ chunk_id: 1, text: "older segment" }),
          entry({ chunk_id: 2, text: "newer segment" }),
        ]}
      />,
    );

    expect(markup.indexOf("newer segment")).toBeGreaterThan(markup.indexOf("older segment"));
  });

  it("always limits the footer to the three newest transcript texts", () => {
    const markup = renderToStaticMarkup(
      <TranscriptFooter
        liveActive
        entries={[
          entry({ chunk_id: 1, status: "final", text: "first transcript" }),
          entry({ chunk_id: 2, status: "final", text: "second transcript" }),
          entry({ chunk_id: 3, status: "final", text: "third transcript" }),
          entry({ chunk_id: 4, status: "final", text: "fourth transcript" }),
        ]}
      />,
    );

    expect(markup).toContain("fourth transcript");
    expect(markup).toContain("third transcript");
    expect(markup).toContain("second transcript");
    expect(markup).not.toContain("first transcript");
  });

  it("keeps text transcripts visible instead of empty processing rows", () => {
    const markup = renderToStaticMarkup(
      <TranscriptFooter
        liveActive
        entries={[
          entry({ chunk_id: 1, status: "listening", text: "" }),
          entry({ chunk_id: 2, status: "queued", text: "" }),
          entry({ chunk_id: 3, status: "final", text: "decoded transcript" }),
        ]}
      />,
    );

    expect(markup).toContain("decoded transcript");
    expect(markup).not.toContain("processing");
    expect(markup).not.toContain("listening to speech");
  });

  it("scrolls the transcript list to the newest bottom row", () => {
    const list = {
      scrollHeight: 480,
      scrollTop: 0,
    };

    scrollTranscriptListToBottom(list);

    expect(list.scrollTop).toBe(480);
  });
});
