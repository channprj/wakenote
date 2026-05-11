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
    recorded_at: "2026-05-09T08:00:00Z",
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
        now={new Date("2026-05-09T08:00:42Z")}
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
        now={new Date("2026-05-09T08:00:42Z")}
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
        now={new Date("2026-05-09T08:00:42Z")}
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
        now={new Date("2026-05-09T08:00:42Z")}
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

  it("shows saved transcripts with timestamp labels and recent age suffixes", () => {
    const recordedAt = new Date(2026, 4, 9, 8, 0, 0).toISOString();
    const markup = renderToStaticMarkup(
      <TranscriptFooter
        liveActive={false}
        now={new Date(2026, 4, 9, 8, 0, 42)}
        entries={[
          entry({
            chunk_id: 7,
            status: "final",
            text: "final transcript",
            recorded_at: recordedAt,
          }),
        ]}
      />,
    );

    expect(markup).toContain("2026-05-09 08:00:00");
    expect(markup).toContain("final transcript 42 seconds ago");
    expect(markup).not.toContain("Saved");
  });

  it("wraps the final-status timestamp in a clickable file:// link to the audio recording", () => {
    const recordedAt = new Date(2026, 4, 9, 8, 0, 0).toISOString();
    const audioPath = "/Users/me/Documents/WakeNote/20260509/080000.m4a";
    const markup = renderToStaticMarkup(
      <TranscriptFooter
        liveActive={false}
        now={new Date(2026, 4, 9, 8, 0, 42)}
        entries={[
          entry({
            chunk_id: 7,
            status: "final",
            text: "final transcript",
            recorded_at: recordedAt,
            audio_path: audioPath,
          }),
        ]}
      />,
    );

    expect(markup).toContain(
      `<a href="file:///Users/me/Documents/WakeNote/20260509/080000.m4a" title="${audioPath}">2026-05-09 08:00:00</a>`,
    );
  });

  it("renders the final-status timestamp without a link when audio_path is null", () => {
    const recordedAt = new Date(2026, 4, 9, 8, 0, 0).toISOString();
    const markup = renderToStaticMarkup(
      <TranscriptFooter
        liveActive={false}
        now={new Date(2026, 4, 9, 8, 0, 42)}
        entries={[
          entry({
            chunk_id: 8,
            status: "final",
            text: "no audio transcript",
            recorded_at: recordedAt,
            audio_path: null,
          }),
        ]}
      />,
    );

    expect(markup).toContain("2026-05-09 08:00:00");
    expect(markup).not.toContain("<a ");
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
