// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings, mockModels } from "@/lib/app-state";
import type { AppSettings, ModelDescriptor } from "@/lib/types";
import {
  TranscriptionOptionsSettings,
  transcriptionFallbackOptions,
} from "./TranscriptionOptionsSettings";

afterEach(cleanup);

function readyLocalModels() {
  return mockModels().map((model) =>
    model.id === "whisper-medium"
      ? { ...model, status: "ready" as const }
      : model,
  );
}

function renderOptions(
  selectedModel: string,
  options: Partial<AppSettings["transcription_options"]> = {},
  models: ModelDescriptor[] = readyLocalModels(),
) {
  const onPatch = vi.fn();
  render(
    <TranscriptionOptionsSettings
      settings={{
        ...defaultSettings(),
        selected_model: selectedModel,
        transcription_options: {
          ...defaultSettings().transcription_options,
          ...options,
        },
      }}
      models={models}
      onPatch={onPatch}
    />,
  );
  return onPatch;
}

describe("TranscriptionOptionsSettings", () => {
  it("locks Streaming according to model capability", () => {
    const cases: Array<
      [
        string,
        Partial<AppSettings["transcription_options"]>,
        "true" | "false",
        RegExp,
      ]
    > = [
      ["openai-gpt-live-transcribe", {}, "true", /required by GPT Live Transcribe/i],
      [
        "openrouter-qwen3-asr-flash",
        { streaming_enabled: true },
        "false",
        /does not support streaming/i,
      ],
    ];

    for (const [model, options, checked, explanation] of cases) {
      cleanup();
      renderOptions(model, options);
      const streaming = screen.getByRole("switch", { name: "Streaming" });
      expect(streaming.getAttribute("aria-checked"), model).toBe(checked);
      expect((streaming as HTMLButtonElement).disabled, model).toBe(true);
      expect(screen.getByText(explanation), model).toBeTruthy();
    }
  });

  it("allows optional Streaming for gpt-transcribe and emits a nested patch", async () => {
    const user = userEvent.setup();
    const onPatch = renderOptions("openai-gpt-transcribe");
    const streaming = screen.getByRole("switch", { name: "Streaming" });
    expect((streaming as HTMLButtonElement).disabled).toBe(false);
    await user.click(streaming);
    expect(onPatch).toHaveBeenCalledWith({
      transcription_options: {
        streaming_enabled: true,
        cost_limit_fallback_enabled: false,
        cost_limit_fallback_model_id: null,
      },
    });
  });

  it("shows only ready on-device fallback choices", () => {
    const choices = transcriptionFallbackOptions(
      readyLocalModels(),
      "openai-gpt-transcribe",
      null,
    );
    expect(choices.options.map((option) => option.label)).toEqual([
      "None",
      "Whisper Medium",
    ]);
  });

  it("keeps an unavailable persisted fallback visible", () => {
    renderOptions("openai-gpt-transcribe", {
      cost_limit_fallback_enabled: true,
      cost_limit_fallback_model_id: "whisper-small",
    });
    expect(screen.getByText("Unavailable")).toBeTruthy();
    expect(screen.getByText("Whisper Small")).toBeTruthy();
    expect(
      screen.getByRole("switch", { name: "Cost-limit fallback" }).getAttribute("aria-checked"),
    ).toBe("false");
  });
});
