import { describe, expect, it } from "vitest";
import { mockModels } from "./app-state";
import {
  availableFallbackModels,
  formatModelLabel,
  normalizeTranscriptionOptions,
} from "./models";
import type { ModelDescriptor } from "./types";

function model(
  id: string,
  displayName: string,
): Pick<ModelDescriptor, "id" | "display_name"> {
  return { id, display_name: displayName };
}

describe("formatModelLabel", () => {
  it("returns the friendly display name when the model is in the list", () => {
    const models = [
      model("whisper-medium", "Whisper Medium"),
      model("whisper-small", "Whisper Small"),
    ];

    expect(formatModelLabel("whisper-medium", models)).toBe("Whisper Medium");
    expect(formatModelLabel("whisper-small", models)).toBe("Whisper Small");
  });

  it("falls back to the raw id when the model is not in the list", () => {
    const models = [model("whisper-medium", "Whisper Medium")];

    expect(formatModelLabel("whisper-removed", models)).toBe("whisper-removed");
  });

  it("falls back to the raw id when the matched display name is blank", () => {
    const models = [model("whisper-blank", "   ")];

    expect(formatModelLabel("whisper-blank", models)).toBe("whisper-blank");
  });

  it("handles an empty model list", () => {
    expect(formatModelLabel("whisper-medium", [])).toBe("whisper-medium");
  });
});

describe("transcription model capabilities", () => {
  it("mirrors the exact OpenAI streaming and diarization matrix", () => {
    const models = mockModels();
    const live = models.find(
      (candidate) => candidate.id === "openai-gpt-live-transcribe",
    );
    const file = models.find(
      (candidate) => candidate.id === "openai-gpt-transcribe",
    );
    const diarize = models.find(
      (candidate) => candidate.id === "openai-gpt-4o-transcribe-diarize",
    );

    expect(live?.capabilities).toMatchObject({
      file_transcription: false,
      realtime: true,
      streaming: "required",
      diarization: false,
    });
    expect(file?.capabilities.streaming).toBe("optional");
    expect(diarize?.capabilities).toMatchObject({
      file_transcription: true,
      realtime: false,
      streaming: "optional",
      diarization: true,
      maximum_request_bytes: 25_000_000,
    });
  });

  it("mirrors the exact Soniox async and realtime capability matrix", () => {
    const models = mockModels();
    const asyncModel = models.find(
      (candidate) => candidate.id === "soniox-async-v5",
    );
    const realtime = models.find(
      (candidate) => candidate.id === "soniox-realtime-v5",
    );

    expect(asyncModel?.capabilities).toMatchObject({
      file_transcription: true,
      realtime: false,
      streaming: "unsupported",
      selectable_contexts: ["file", "dictation", "meeting"],
    });
    expect(realtime?.capabilities).toMatchObject({
      file_transcription: false,
      realtime: true,
      streaming: "required",
      selectable_contexts: ["realtime", "dictation"],
    });
  });

  it("locks required streaming on and unsupported streaming off", () => {
    const models = mockModels();
    const base = {
      streaming_enabled: false,
      cost_limit_fallback_enabled: false,
      cost_limit_fallback_model_id: null,
    };
    const live = models.find(
      (candidate) => candidate.id === "openai-gpt-live-transcribe",
    );
    const openrouter = models.find(
      (candidate) => candidate.id === "openrouter-qwen3-asr-flash",
    );

    expect(
      normalizeTranscriptionOptions(live, models, base).streaming_enabled,
    ).toBe(true);
    expect(
      normalizeTranscriptionOptions(openrouter, models, {
        ...base,
        streaming_enabled: true,
      }).streaming_enabled,
    ).toBe(false);
  });

  it("offers only ready on-device file models for fallback", () => {
    const models = mockModels().map((candidate) =>
      candidate.id === "whisper-medium"
        ? { ...candidate, status: "ready" as const }
        : candidate,
    );
    expect(
      availableFallbackModels(models).map((candidate) => candidate.id),
    ).toEqual(["whisper-medium"]);

    const primary = models.find(
      (candidate) => candidate.id === "openai-gpt-transcribe",
    );
    const normalized = normalizeTranscriptionOptions(primary, models, {
      streaming_enabled: false,
      cost_limit_fallback_enabled: true,
      cost_limit_fallback_model_id: "whisper-small",
    });
    expect(normalized.cost_limit_fallback_enabled).toBe(false);
    expect(normalized.cost_limit_fallback_model_id).toBe("whisper-small");
  });
});
