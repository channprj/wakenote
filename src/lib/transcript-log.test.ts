import { describe, expect, it } from "vitest";
import {
  TRANSCRIPT_LOG_LIMIT,
  reduceTranscriptLog,
  type TranscriptEntry,
  type TranscriptEvent,
} from "./transcript-log";

function apply(events: TranscriptEvent[]): TranscriptEntry[] {
  return events.reduce<TranscriptEntry[]>(
    (entries, event) => reduceTranscriptLog(entries, event),
    [],
  );
}

describe("reduceTranscriptLog", () => {
  it("keeps equal local chunk ids from distinct microphones", () => {
    const entries = apply([
      {
        type: "started",
        source_key: "microphone:wired",
        source_label: "Wired",
        microphone_slot: "primary",
        chunk_id: 1,
        started_at: "2026-05-09T08:00:00Z",
      },
      {
        type: "started",
        source_key: "microphone:wireless",
        source_label: "Wireless",
        microphone_slot: "secondary",
        chunk_id: 1,
        started_at: "2026-05-09T08:00:01Z",
      },
    ]);

    expect(entries).toHaveLength(2);
    expect(entries.map((entry) => entry.source_label)).toEqual([
      "Wired",
      "Wireless",
    ]);
  });

  it("creates a listening entry on started and upgrades through partial -> queued -> final", () => {
    const entries = apply([
      { type: "started", chunk_id: 1, started_at: "2026-05-09T08:00:00Z" },
      { type: "partial", chunk_id: 1, text: "hello" },
      { type: "committed", chunk_id: 1, audio_path: "/tmp/1.m4a" },
      {
        type: "final",
        chunk_id: 1,
        audio_path: "/tmp/1.m4a",
        text: "hello world",
      },
    ]);

    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      chunk_id: 1,
      status: "final",
      text: "hello world",
      audio_path: "/tmp/1.m4a",
    });
  });

  it("ignores partials that arrive after the chunk has already finalized", () => {
    const entries = apply([
      { type: "started", chunk_id: 1, started_at: "2026-05-09T08:00:00Z" },
      {
        type: "final",
        chunk_id: 1,
        audio_path: "/tmp/1.m4a",
        text: "final text",
      },
      { type: "partial", chunk_id: 1, text: "stale partial" },
    ]);

    expect(entries[0].status).toBe("final");
    expect(entries[0].text).toBe("final text");
  });

  it("creates a visible partial entry even if the started event was missed", () => {
    const entries = apply([
      { type: "partial", chunk_id: 42, text: "실시간 전사 미리보기" },
    ]);

    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      chunk_id: 42,
      status: "partial",
      text: "실시간 전사 미리보기",
    });
  });

  it("keeps a late live partial visible after the chunk is committed", () => {
    const entries = apply([
      { type: "started", chunk_id: 3, started_at: "2026-05-09T08:00:00Z" },
      { type: "committed", chunk_id: 3, audio_path: "/tmp/3.m4a" },
      {
        type: "partial",
        chunk_id: 3,
        text: "한국어 회의 내용을 실시간으로 표시합니다.",
      },
    ]);

    expect(entries[0]).toMatchObject({
      chunk_id: 3,
      status: "partial",
      text: "한국어 회의 내용을 실시간으로 표시합니다.",
      audio_path: "/tmp/3.m4a",
    });
  });

  it("matches finals to existing entries by audio_path when chunk_id is unknown", () => {
    const entries = apply([
      { type: "started", chunk_id: 7, started_at: "2026-05-09T08:00:00Z" },
      { type: "committed", chunk_id: 7, audio_path: "/tmp/7.m4a" },
      {
        type: "final",
        chunk_id: null,
        audio_path: "/tmp/7.m4a",
        text: "from backlog",
      },
    ]);

    expect(entries).toHaveLength(1);
    expect(entries[0].status).toBe("final");
    expect(entries[0].text).toBe("from backlog");
  });

  it("uses the recording timestamp from delayed final events when no live entry remains", () => {
    const recordedAt = "2026-05-09T08:00:00Z";
    const entries = apply([
      {
        type: "final",
        chunk_id: 42,
        audio_path: "/tmp/WakeNote/20260509/080000.m4a",
        text: "delayed queue result",
        recorded_at: recordedAt,
      },
    ]);

    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      chunk_id: 42,
      status: "final",
      started_at: recordedAt,
      recorded_at: recordedAt,
      text: "delayed queue result",
    });
  });

  it("records failed status with error message", () => {
    const entries = apply([
      { type: "started", chunk_id: 2, started_at: "2026-05-09T08:00:00Z" },
      { type: "committed", chunk_id: 2, audio_path: "/tmp/2.m4a" },
      {
        type: "failed",
        chunk_id: 2,
        audio_path: "/tmp/2.m4a",
        error: "model missing",
      },
    ]);

    expect(entries[0]).toMatchObject({
      status: "failed",
      error: "model missing",
    });
  });

  it("trims to at most TRANSCRIPT_LOG_LIMIT entries", () => {
    const events: TranscriptEvent[] = Array.from(
      { length: TRANSCRIPT_LOG_LIMIT + 5 },
      (_, index) => ({
        type: "started" as const,
        chunk_id: index + 1,
        started_at: "2026-05-09T08:00:00Z",
      }),
    );
    const entries = apply(events);

    expect(entries).toHaveLength(TRANSCRIPT_LOG_LIMIT);
    expect(entries[0].chunk_id).toBe(6);
    expect(entries.at(-1)?.chunk_id).toBe(TRANSCRIPT_LOG_LIMIT + 5);
  });
});
