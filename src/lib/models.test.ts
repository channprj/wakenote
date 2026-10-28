import { describe, expect, it } from "vitest";
import { mockModels } from "./app-state";
import {
  availableFallbackModels,
  formatModelLabel,
  groupModelsByLocation,
  meetingTranscriptionModels,
  normalizeTranscriptionOptions,
  resolveMeetingModelId,
} from "./models";
import type { ModelDescriptor } from "./types";

function model(
  id: string,
  displayName: string,
): Pick<ModelDescriptor, "id" | "display_name"> {
  return { id, display_name: displayName };
}

describe("formatModelLabel", () => {
  it("formats a model label from its display name or falls back to the id", () => {
    const listed = [
      model("whisper-medium", "Whisper Medium"),
      model("whisper-small", "Whisper Small"),
    ];
    const cases: Array<[string, Parameters<typeof formatModelLabel>[1], string]> = [
      // Friendly display name when the model is in the list.
      ["whisper-medium", listed, "Whisper Medium"],
      ["whisper-small", listed, "Whisper Small"],
      // Raw id when the model is not in the list.
      ["whisper-removed", [model("whisper-medium", "Whisper Medium")], "whisper-removed"],
      // Raw id when the matched display name is blank.
      ["whisper-blank", [model("whisper-blank", "   ")], "whisper-blank"],
      // Empty model list.
      ["whisper-medium", [], "whisper-medium"],
    ];

    for (const [id, models, expected] of cases) {
      expect(formatModelLabel(id, models), `${id} in ${models.length} models`).toBe(expected);
    }
  });
});

describe("groupModelsByLocation", () => {
  it("keeps local models before API models without changing group order", () => {
    const models = mockModels();
    const groups = groupModelsByLocation(models);

    expect(groups.map((group) => group.label)).toEqual([
      "Local models",
      "API models",
    ]);
    expect(groups[0]?.models.every((candidate) => candidate.offline)).toBe(
      true,
    );
    expect(groups[1]?.models.every((candidate) => !candidate.offline)).toBe(
      true,
    );
    expect(groups.flatMap((group) => group.models)).toHaveLength(models.length);
  });

  it("omits empty groups", () => {
    const localModels = mockModels().filter((candidate) => candidate.offline);

    expect(
      groupModelsByLocation(localModels).map((group) => group.label),
    ).toEqual(["Local models"]);
  });
});

describe("transcription model capabilities", () => {
  it("resolves runnable file, realtime, and unloaded meeting models", () => {
    const models = mockModels().map((candidate) =>
      candidate.id === "whisper-medium"
        ? { ...candidate, status: "unloaded" as const }
        : candidate,
    );

    expect(
      meetingTranscriptionModels(models).map((candidate) => candidate.id),
    ).toEqual(
      expect.arrayContaining([
        "whisper-medium",
        "openai-gpt-live-transcribe",
        "soniox-realtime-v5",
      ]),
    );
    expect(
      resolveMeetingModelId("soniox-async-v5", "soniox-realtime-v5", models),
    ).toBe("soniox-async-v5");
    expect(resolveMeetingModelId("", "soniox-realtime-v5", models)).toBe(
      "soniox-realtime-v5",
    );

    expect(
      meetingTranscriptionModels(
        models.map((candidate) =>
          candidate.id === "whisper-medium"
            ? { ...candidate, status: "downloading" as const }
            : candidate,
        ),
      ).map((candidate) => candidate.id),
    ).not.toContain("whisper-medium");
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
