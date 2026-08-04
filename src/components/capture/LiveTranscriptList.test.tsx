import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TranscriptEntry } from "@/lib/transcript-log";
import { LiveTranscriptList } from "./LiveTranscriptList";

function entry(overrides: Partial<TranscriptEntry>): TranscriptEntry {
  return {
    chunk_id: 1,
    status: "partial",
    text: "현재 문장",
    started_at: "2026-07-15T00:00:00Z",
    recorded_at: "2026-07-15T00:00:00Z",
    audio_path: null,
    error: null,
    ...overrides,
  };
}

describe("LiveTranscriptList", () => {
  it("renders live text in an in-flow semantic list", () => {
    const markup = renderToStaticMarkup(
      <LiveTranscriptList
        entries={[entry({ status: "partial" })]}
        liveActive
      />,
    );

    expect(markup).toContain(
      '<ol data-slot="live-transcript-list" aria-label="Live transcription"',
    );
    expect(markup).toContain('<li data-status="partial">');
    expect(markup).toContain('data-slot="transcript-meta"');
    expect(markup).toContain(">Live</span>");
    expect(markup).toContain('data-slot="transcript-text"');
    expect(markup).toContain("현재 문장");
  });

  it("renders microphone names for interleaved live entries", () => {
    const markup = renderToStaticMarkup(
      <LiveTranscriptList
        liveActive
        entries={[
          entry({
            source_key: "microphone:wired",
            source_label: "Wired",
            microphone_slot: "primary",
          }),
          entry({
            source_key: "microphone:wireless",
            source_label: "Wireless",
            microphone_slot: "secondary",
            chunk_id: 1,
            started_at: "2026-07-15T00:00:01Z",
          }),
        ]}
      />,
    );

    expect(markup).toContain('data-slot="transcript-source">Wired</span>');
    expect(markup).toContain('data-slot="transcript-source">Wireless</span>');
  });

  it("links final timestamps to audio and keeps the full transcript visible", () => {
    const recordedAt = new Date(2026, 6, 15, 2, 30, 0).toISOString();
    const audioPath = "/Users/me/Documents/WakeNote/20260715/023000.m4a";
    const markup = renderToStaticMarkup(
      <LiveTranscriptList
        liveActive={false}
        now={new Date(2026, 6, 15, 2, 30, 42)}
        entries={[
          entry({
            status: "final",
            text: "길이가 매우 긴 한국어 전사",
            recorded_at: recordedAt,
            audio_path: audioPath,
          }),
        ]}
      />,
    );

    expect(markup).toContain(`href="file://${audioPath}"`);
    expect(markup).toContain("2026-07-15 02:30:00");
    expect(markup).toContain("길이가 매우 긴 한국어 전사 42 seconds ago");
  });

  it("uses a real empty state without synthetic transcript content", () => {
    const markup = renderToStaticMarkup(
      <LiveTranscriptList entries={[]} liveActive={false} />,
    );

    expect(markup).toContain("No speech captured yet");
    expect(markup).not.toContain("data-status=");
  });

  it("shows an empty listening lifecycle row while waiting for speech", () => {
    const markup = renderToStaticMarkup(
      <LiveTranscriptList
        entries={[entry({ status: "listening", text: "" })]}
        liveActive
      />,
    );

    expect(markup).toContain('<li data-status="listening">');
    expect(markup).toContain("Listening");
    expect(markup).toContain("Waiting for speech");
  });

  it("keeps the last preview visible when finalization needs review", () => {
    const markup = renderToStaticMarkup(
      <LiveTranscriptList
        entries={[
          entry({
            status: "failed",
            text: "last useful preview",
            error: "final pass failed",
          }),
        ]}
        liveActive
      />,
    );

    expect(markup).toContain("Review");
    expect(markup).toContain("last useful preview");
  });
});
