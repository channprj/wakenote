import { describe, expect, it } from "vitest";
import type { TranscriptEntry } from "./transcript-log";
import { newestTranscriptTextEntries } from "./live-transcripts";

function entry(overrides: Partial<TranscriptEntry>): TranscriptEntry {
  return {
    chunk_id: 1,
    status: "partial",
    text: "",
    started_at: "2026-07-15T00:00:00Z",
    recorded_at: "2026-07-15T00:00:00Z",
    audio_path: null,
    error: null,
    ...overrides,
  };
}

describe("newestTranscriptTextEntries", () => {
  it("keeps only the newest three distinct non-failed transcript texts", () => {
    const selected = newestTranscriptTextEntries([
      entry({ chunk_id: 1, text: "first" }),
      entry({ chunk_id: 2, text: "second", audio_path: "/second.m4a" }),
      entry({ chunk_id: 3, text: "second", audio_path: "/duplicate.m4a" }),
      entry({ chunk_id: 4, status: "failed", text: "failed" }),
      entry({ chunk_id: 5, text: "third" }),
      entry({ chunk_id: 6, text: "fourth" }),
    ]);

    expect(selected.map((item) => item.text)).toEqual(["second", "third", "fourth"]);
  });
});
