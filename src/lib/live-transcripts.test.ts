import { describe, expect, it } from "vitest";
import type { TranscriptEntry } from "./transcript-log";
import {
  newestLiveTranscriptEntries,
  newestTranscriptTextEntries,
  livePreviewPresentation,
} from "./live-transcripts";
import { mockModels } from "./app-state";

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

describe("newestLiveTranscriptEntries", () => {
  it("retains empty listening and failed lifecycle rows", () => {
    const selected = newestLiveTranscriptEntries([
      entry({ chunk_id: 1, status: "listening", text: "" }),
      entry({
        chunk_id: 2,
        status: "failed",
        text: "last useful preview",
        error: "final pass failed",
      }),
    ]);

    expect(selected.map((item) => item.status)).toEqual([
      "listening",
      "failed",
    ]);
  });

  it("keeps equal text from distinct sources as separate live rows", () => {
    const selected = newestLiveTranscriptEntries([
      entry({ chunk_id: 1, source_key: "microphone:wired", text: "hello" }),
      entry({
        chunk_id: 1,
        source_key: "microphone:wireless",
        text: "hello",
      }),
    ]);

    expect(selected).toHaveLength(2);
  });
});

describe("livePreviewPresentation", () => {
  it("distinguishes a provisional local preview from the selected final model", () => {
    const models = mockModels();
    const selected = models.find((model) => model.id === "qwen3-asr-1.7b")!;
    selected.status = "ready";
    const preview = models.find((model) => model.id === "whisper-turbo")!;
    preview.status = "ready";

    expect(livePreviewPresentation(selected.id, models)).toEqual({
      previewModelId: "whisper-turbo",
      provisional: true,
      description:
        "Live preview uses Whisper Turbo; final transcription remains Qwen3-ASR 1.7B.",
    });
  });

  it("explains when only final transcription is available", () => {
    const models = mockModels();
    const selected = models.find(
      (model) => model.id === "openrouter-qwen3-asr-flash",
    )!;
    selected.status = "ready";

    expect(livePreviewPresentation(selected.id, models)).toEqual({
      previewModelId: null,
      provisional: false,
      description:
        "Live preview waits for a compatible model; final transcription uses OpenRouter · Qwen3 ASR Flash.",
    });
  });
});
