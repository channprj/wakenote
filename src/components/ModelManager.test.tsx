import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { defaultSettings, mockModels } from "../lib/app-state";
import type { ModelDescriptor, ModelStatus } from "../lib/types";
import {
  ModelManager,
  formatLanguageList,
  formatModelSize,
  modelAcquireAction,
  modelCancelDownloadDisabledReason,
  modelCapabilityLabels,
  modelDeleteDisabledReason,
  modelDownloadDisabledReason,
  modelRetryDisabledReason,
  modelSwitchDisabledReason,
  modelVerifyDisabledReason,
} from "./ModelManager";

function renderModel(
  status: ModelStatus,
  overrides: Partial<ModelDescriptor> = {},
  selectedModel = "other-model",
  dictationModel = "",
  meetingModel = "",
) {
  const model: ModelDescriptor = {
    ...mockModels()[0],
    status,
    download_url: "https://example.invalid/model.bin",
    ...overrides,
  };

  return renderToStaticMarkup(
    <ModelManager
      models={[model]}
      settings={{
        ...defaultSettings(),
        selected_model: selectedModel,
        dictation_model: dictationModel,
        meeting_model: meetingModel,
      }}
      onPatch={() => {}}
      onVerify={() => {}}
      onDownload={() => {}}
      onCancelDownload={() => {}}
      onDelete={() => {}}
    />,
  );
}

function buttonTag(markup: string, iconClass: string) {
  const re = new RegExp(
    `<button(?:(?!</button>)[\\s\\S])*?lucide-${iconClass}(?:(?!</button>)[\\s\\S])*?</button>`,
  );
  const match = markup.match(re);
  expect(
    match,
    `expected button containing lucide-${iconClass}`,
  ).not.toBeNull();
  return match?.[0] ?? "";
}

function buttonWithText(markup: string, text: string) {
  const match = markup.match(
    new RegExp(`<button[^>]*>[\\s\\S]*?${text}[\\s\\S]*?</button>`),
  );
  expect(match, `expected ${text} button`).not.toBeNull();
  return match?.[0] ?? "";
}

function isDisabled(button: string) {
  return /\sdisabled(=""|\s|>)/.test(button);
}

function titleOf(button: string): string | null {
  const match = button.match(/\stitle="([^"]*)"/);
  return match ? match[1] : null;
}

describe("model card grid", () => {
  it("separates on-device and API models into labeled groups", () => {
    const models = mockModels();
    const markup = renderToStaticMarkup(
      <ModelManager
        models={models}
        settings={defaultSettings()}
        onPatch={() => {}}
        onVerify={() => {}}
        onDownload={() => {}}
        onCancelDownload={() => {}}
        onDelete={() => {}}
      />,
    );

    const localGroup = markup.match(
      /<section[^>]*data-model-kind="local"[\s\S]*?<\/section>/,
    )?.[0];
    const apiGroup = markup.match(
      /<section[^>]*data-model-kind="api"[\s\S]*?<\/section>/,
    )?.[0];

    expect(localGroup).toContain("Local models");
    expect(apiGroup).toContain("API models");
    for (const model of models.filter((candidate) => candidate.offline)) {
      expect(localGroup).toContain(model.display_name);
      expect(apiGroup).not.toContain(model.display_name);
    }
    for (const model of models.filter((candidate) => !candidate.offline)) {
      expect(apiGroup).toContain(model.display_name);
      expect(localGroup).not.toContain(model.display_name);
    }
    expect(markup.indexOf('data-model-kind="local"')).toBeLessThan(
      markup.indexOf('data-model-kind="api"'),
    );
  });

  it("renders every model as a card in one capped responsive collection", () => {
    const models = mockModels();
    const markup = renderToStaticMarkup(
      <ModelManager
        models={models}
        settings={defaultSettings()}
        onPatch={() => {}}
        onVerify={() => {}}
        onDownload={() => {}}
        onCancelDownload={() => {}}
        onDelete={() => {}}
      />,
    );

    expect(markup).toContain('data-slot="model-card-grid"');
    expect(markup).toContain('data-max-columns="3"');
    expect(markup.match(/class="model-row"/g)).toHaveLength(models.length);
    expect(markup).toContain('role="list"');
    expect(markup).toContain('role="listitem"');
    expect(markup).toContain(
      'aria-label="Whisper Small installation progress"',
    );
  });
});

describe("model manager actions", () => {
  it("enables card actions according to model status", () => {
    type CardButton = "switch" | "download" | "verify" | "cancel" | "delete";
    const buttonFor: Record<CardButton, (markup: string) => string> = {
      switch: (markup) => buttonWithText(markup, "Switch"),
      download: (markup) => buttonTag(markup, "download"),
      verify: (markup) => buttonTag(markup, "shield-check"),
      cancel: (markup) => buttonTag(markup, "circle-x"),
      delete: (markup) => buttonTag(markup, "trash-2"),
    };
    // Each row lists whether a card button is disabled; omitted buttons are not asserted.
    const cases: Array<{
      label: string;
      status: ModelStatus;
      overrides?: Partial<ModelDescriptor>;
      disabled: Partial<Record<CardButton, boolean>>;
    }> = [
      {
        label: "ready",
        status: "ready",
        disabled: { switch: false, download: true, verify: false, delete: false },
      },
      {
        label: "installed",
        status: "installed",
        disabled: { switch: false, download: true, verify: false, delete: false },
      },
      {
        label: "unloaded",
        status: "unloaded",
        disabled: { switch: false, download: true, verify: false, delete: false },
      },
      {
        label: "downloading",
        status: "downloading",
        disabled: {
          switch: true,
          download: true,
          verify: true,
          cancel: false,
          delete: true,
        },
      },
      {
        label: "verifying",
        status: "verifying",
        disabled: { download: true, verify: true, cancel: false, delete: true },
      },
      {
        label: "extracting",
        status: "extracting",
        disabled: { download: true, verify: true, cancel: false, delete: true },
      },
      {
        label: "error",
        status: "error",
        disabled: { switch: true, verify: false, delete: false },
      },
      {
        label: "missing",
        status: "missing",
        disabled: { switch: true, download: false, verify: false, delete: true },
      },
      {
        label: "missing without a download URL",
        status: "missing",
        overrides: { download_url: null },
        disabled: { download: true },
      },
    ];

    for (const { label, status, overrides, disabled } of cases) {
      const markup = renderModel(status, overrides);
      for (const [button, expected] of Object.entries(disabled) as Array<
        [CardButton, boolean]
      >) {
        expect(
          isDisabled(buttonFor[button](markup)),
          `${label}: ${button} disabled`,
        ).toBe(expected);
      }
    }
  });

  it("routes failed downloads to Retry with explanatory titles", () => {
    const markup = renderModel("error", {
      download_url: "https://example.invalid/m.bin",
      download_error: "network unavailable",
    });

    expect(isDisabled(buttonTag(markup, "download"))).toBe(true);
    expect(titleOf(buttonTag(markup, "download"))).toBe(
      "Download failed — use Retry",
    );
    expect(isDisabled(buttonTag(markup, "rotate-cw"))).toBe(false);
    expect(titleOf(buttonTag(markup, "rotate-cw"))).toBe("Retry");
    expect(titleOf(buttonWithText(markup, "Switch"))).toBe(
      "Model has a download error",
    );
  });

  it("does not offer retry for failed models without a download URL", () => {
    const markup = renderModel("error", {
      download_url: null,
      download_error: "manual model path is missing",
    });

    expect(isDisabled(buttonTag(markup, "download"))).toBe(true);
    expect(isDisabled(buttonTag(markup, "rotate-cw"))).toBe(true);
  });

  it("blocks deleting a model used by transcription, Dictation, or Meetings", () => {
    const modelId = mockModels()[0].id;
    const cases: Array<[string, string, string, string]> = [
      ["selected model", modelId, "", ""],
      ["Dictation model", "other-model", modelId, ""],
      ["Meetings model", "other-model", "", modelId],
    ];

    for (const [label, selectedModel, dictationModel, meetingModel] of cases) {
      const markup = renderModel(
        "ready",
        {},
        selectedModel,
        dictationModel,
        meetingModel,
      );

      expect(isDisabled(buttonTag(markup, "trash-2")), label).toBe(true);
      expect(titleOf(buttonTag(markup, "trash-2")), label).toBe(
        "Cannot delete a model used by transcription, Dictation, or Meetings",
      );
    }
  });

  it("keeps cloud models selectable without local file actions", () => {
    const cases: Array<[string, string]> = [
      ["openrouter-stt", "Requires OpenRouter API key"],
      ["openai-stt", "Requires OpenAI API key"],
      ["openai-realtime", "Requires OpenAI API key"],
      ["soniox-async-stt", "Requires Soniox API key"],
      ["soniox-realtime", "Requires Soniox API key"],
    ];

    for (const [providerRuntime, label] of cases) {
      const markup = renderModel("ready", {
        provider_runtime: providerRuntime,
        offline: false,
        download_url: null,
        size_mb: 0,
      });

      expect(markup, providerRuntime).toContain(">API<");
      expect(markup, providerRuntime).toContain(label);
      expect(
        isDisabled(buttonWithText(markup, "Switch")),
        `${providerRuntime}: Switch disabled`,
      ).toBe(false);
      expect(markup, providerRuntime).not.toContain("lucide-download");
      expect(markup, providerRuntime).not.toContain("lucide-shield-check");
      expect(markup, providerRuntime).not.toContain("lucide-trash-2");
    }
  });

  it("renders only declared capability badges", () => {
    const live = mockModels().find(
      (model) => model.id === "openai-gpt-live-transcribe",
    );
    const diarize = mockModels().find(
      (model) => model.id === "openai-gpt-4o-transcribe-diarize",
    );
    const local = mockModels().find((model) => model.id === "whisper-medium");

    expect(modelCapabilityLabels(live!)).toEqual([
      "Realtime",
      "Streaming required",
      "API",
    ]);
    expect(modelCapabilityLabels(diarize!)).toEqual([
      "Streaming",
      "Speaker separation",
      "API",
    ]);
    expect(modelCapabilityLabels(local!)).toEqual(["On-device"]);
  });
});

describe("model acquire action", () => {
  it("derives the acquire action per runtime and URL", () => {
    const cases: Array<
      [
        string,
        Pick<ModelDescriptor, "provider_runtime" | "download_url" | "status">,
        Record<string, unknown>,
      ]
    > = [
      [
        "sherpa-onnx with a URL",
        {
          provider_runtime: "sherpa-onnx",
          download_url: "https://example.invalid/model.tar.bz2",
          status: "missing",
        },
        { kind: "download", enabled: true },
      ],
      [
        "whisper-rs with a URL",
        {
          provider_runtime: "whisper-rs",
          download_url: "https://example.invalid/m.bin",
          status: "missing",
        },
        { kind: "download", enabled: true },
      ],
      [
        "whisper-rs without a URL",
        {
          provider_runtime: "whisper-rs",
          download_url: null,
          status: "missing",
        },
        {
          kind: "download",
          enabled: false,
          reason: "No download URL available",
        },
      ],
      [
        "built-in Qwen3-ASR",
        {
          provider_runtime: "qwen3-asr",
          download_url: null,
          status: "missing",
        },
        {
          kind: "download",
          enabled: true,
          label: "Set up Qwen3-ASR",
          reason: null,
        },
      ],
    ];

    for (const [label, model, expected] of cases) {
      expect(modelAcquireAction(model), label).toMatchObject(expected);
    }
  });
});

describe("formatModelSize", () => {
  it("formats model sizes as MB, GB, or a placeholder", () => {
    const cases: Array<[number, string]> = [
      // Below 1 GiB renders as MB.
      [75, "75 MB"],
      [465, "465 MB"],
      [1023, "1023 MB"],
      // At or above 1 GiB renders as GB with one decimal.
      [1024, "1.0 GB"],
      [1031, "1.0 GB"],
      [1465, "1.4 GB"],
      [1550, "1.5 GB"],
      // Invalid sizes render as an em-dash placeholder.
      [Number.NaN, "—"],
      [-1, "—"],
    ];

    for (const [sizeMb, expected] of cases) {
      expect(formatModelSize(sizeMb), `formatModelSize(${sizeMb})`).toBe(
        expected,
      );
    }
  });

  it("is rendered into the model row metadata strip", () => {
    const markup = renderModel("ready", { size_mb: 1465 });

    expect(markup).toContain("1.4 GB");
    expect(markup).not.toContain("1465 MB");
  });
});

describe("model download error rendering", () => {
  it("wraps download_error in a danger-toned span when present", () => {
    const markup = renderModel("error", {
      download_error: "network unavailable",
    });

    expect(markup).toContain(
      '<span data-tone="danger">network unavailable</span>',
    );
  });

  it("omits the toned span when download_error is null", () => {
    const markup = renderModel("ready", { download_error: null });

    expect(markup).not.toContain('data-tone="danger"');
  });
});

describe("formatLanguageList", () => {
  it("formats language code lists into deduplicated labels", () => {
    const cases: Array<[string[], string]> = [
      // Known codes map to human-readable labels.
      [["ko", "en", "multi"], "Korean, English, Multilingual"],
      [["ja", "zh"], "Japanese, Chinese"],
      // Unknown codes are preserved as-is.
      [["xx", "en"], "xx, English"],
      // Codes that map to the same name are not repeated.
      [["en", "en"], "English"],
    ];

    for (const [codes, expected] of cases) {
      expect(formatLanguageList(codes), codes.join(",")).toBe(expected);
    }
  });

  it("renders friendly labels in the model row", () => {
    const markup = renderModel("ready", { languages: ["ko", "en", "multi"] });

    expect(markup).toContain("Korean, English, Multilingual");
    expect(markup).not.toContain(">ko, en, multi<");
  });
});

describe("model switch disabled reason", () => {
  it("describes switch availability for every status and selection", () => {
    const cases: Array<[ModelStatus, boolean, string | null]> = [
      ["ready", false, null],
      ["installed", false, null],
      ["unloaded", false, null],
      ["downloading", false, "Model is still downloading"],
      ["verifying", false, "Model is still downloading"],
      ["extracting", false, "Model is still downloading"],
      ["missing", false, "Download the model before switching"],
      ["error", false, "Model has a download error"],
      ["missing", true, null],
      ["error", true, null],
    ];

    for (const [status, isSelected, reason] of cases) {
      expect(
        modelSwitchDisabledReason(
          { status, provider_runtime: "whisper-rs" },
          isSelected,
        ),
        `${status} (selected=${isSelected})`,
      ).toBe(reason);
    }
  });
});

describe("model download disabled reason", () => {
  it("describes download availability for every status and URL", () => {
    const cases: Array<[ModelStatus, string | null, string | null]> = [
      ["missing", "https://example.invalid/m.bin", null],
      ["downloading", "https://x", "Download already in progress"],
      ["verifying", "https://x", "Download already in progress"],
      ["extracting", "https://x", "Download already in progress"],
      ["ready", "https://x", "Model is already installed"],
      ["installed", "https://x", "Model is already installed"],
      ["unloaded", "https://x", "Model is already installed"],
      ["error", "https://x", "Download failed — use Retry"],
      ["missing", null, "No download URL available"],
      // A missing URL is the most actionable reason, even for error status.
      ["error", null, "No download URL available"],
    ];

    for (const [status, download_url, reason] of cases) {
      expect(
        modelDownloadDisabledReason({ status, download_url }),
        `${status} (download_url=${download_url})`,
      ).toBe(reason);
    }
  });
});

describe("model verify disabled reason", () => {
  it("describes verify availability for every status", () => {
    const cases: Array<[ModelStatus, string | null]> = [
      ["downloading", "Download in progress"],
      ["verifying", "Download in progress"],
      ["extracting", "Download in progress"],
      ["ready", null],
      ["installed", null],
      ["unloaded", null],
      ["missing", null],
      ["error", null],
    ];

    for (const [status, reason] of cases) {
      expect(modelVerifyDisabledReason({ status }), status).toBe(reason);
    }
  });
});

describe("model retry disabled reason", () => {
  it("describes retry availability for every status and URL", () => {
    const cases: Array<[ModelStatus, string | null, string | null]> = [
      ["error", "https://x", null],
      ["downloading", "https://x", "Download already in progress"],
      ["verifying", "https://x", "Download already in progress"],
      ["extracting", "https://x", "Download already in progress"],
      ["error", null, "No download URL available"],
      ["ready", "https://x", "Nothing to retry"],
      ["installed", "https://x", "Nothing to retry"],
      ["unloaded", "https://x", "Nothing to retry"],
      ["missing", "https://x", "Nothing to retry"],
    ];

    for (const [status, download_url, reason] of cases) {
      expect(
        modelRetryDisabledReason({ status, download_url }),
        `${status} (download_url=${download_url})`,
      ).toBe(reason);
    }
  });
});

describe("model cancel download disabled reason", () => {
  it("describes cancel-download availability for every status", () => {
    const cases: Array<[ModelStatus, string | null]> = [
      ["downloading", null],
      ["verifying", null],
      ["extracting", null],
      ["ready", "No active download"],
      ["installed", "No active download"],
      ["unloaded", "No active download"],
      ["missing", "No active download"],
      ["error", "No active download"],
    ];

    for (const [status, reason] of cases) {
      expect(modelCancelDownloadDisabledReason({ status }), status).toBe(
        reason,
      );
    }
  });
});

describe("model delete disabled reason", () => {
  it("describes delete availability for every status and usage", () => {
    const inUseReason =
      "Cannot delete a model used by transcription, Dictation, or Meetings";
    const cases: Array<[ModelStatus, boolean, string | null]> = [
      ["ready", true, inUseReason],
      ["installed", true, inUseReason],
      ["unloaded", true, inUseReason],
      ["error", true, inUseReason],
      ["downloading", false, "Download in progress"],
      ["verifying", false, "Download in progress"],
      ["extracting", false, "Download in progress"],
      ["missing", false, "Model is not downloaded"],
      ["ready", false, null],
      ["installed", false, null],
      ["unloaded", false, null],
      ["error", false, null],
    ];

    for (const [status, isInUse, reason] of cases) {
      expect(
        modelDeleteDisabledReason({ status }, isInUse),
        `${status} (inUse=${isInUse})`,
      ).toBe(reason);
    }
  });
});

describe("disabled-reason titles render on the action buttons", () => {
  it("surfaces the download/verify/cancel/delete reasons while downloading", () => {
    const markup = renderModel("downloading");

    expect(titleOf(buttonTag(markup, "download"))).toBe(
      "Download already in progress",
    );
    expect(titleOf(buttonTag(markup, "shield-check"))).toBe(
      "Download in progress",
    );
    expect(titleOf(buttonTag(markup, "circle-x"))).toBe("Cancel Download");
    expect(titleOf(buttonTag(markup, "trash-2"))).toBe("Download in progress");
    expect(titleOf(buttonTag(markup, "rotate-cw"))).toBe(
      "Download already in progress",
    );
  });

  it("flags missing-without-url for both Download and Retry", () => {
    const markup = renderModel("missing", { download_url: null });
    expect(titleOf(buttonTag(markup, "download"))).toBe(
      "No download URL available",
    );
    expect(titleOf(buttonTag(markup, "rotate-cw"))).toBe("Nothing to retry");
    expect(titleOf(buttonTag(markup, "trash-2"))).toBe(
      "Model is not downloaded",
    );
  });

  it("keeps action-name fallbacks on enabled buttons (no Switch reason when usable)", () => {
    const markup = renderModel("ready");
    expect(titleOf(buttonWithText(markup, "Switch"))).toBeNull();
    expect(titleOf(buttonTag(markup, "shield-check"))).toBe("Verify");
    expect(titleOf(buttonTag(markup, "trash-2"))).toBe("Delete");
    expect(titleOf(buttonTag(markup, "circle-x"))).toBe("No active download");
  });
});
