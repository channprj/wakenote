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
  expect(match, `expected button containing lucide-${iconClass}`).not.toBeNull();
  return match?.[0] ?? "";
}

function buttonWithText(markup: string, text: string) {
  const match = markup.match(new RegExp(`<button[^>]*>[\\s\\S]*?${text}[\\s\\S]*?</button>`));
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
  });
});

describe("model manager actions", () => {
  it("enables switching only for usable local models", () => {
    expect(isDisabled(buttonWithText(renderModel("ready"), "Switch"))).toBe(false);
    expect(isDisabled(buttonWithText(renderModel("installed"), "Switch"))).toBe(false);
    expect(isDisabled(buttonWithText(renderModel("unloaded"), "Switch"))).toBe(false);
    expect(isDisabled(buttonWithText(renderModel("missing"), "Switch"))).toBe(true);
    expect(isDisabled(buttonWithText(renderModel("downloading"), "Switch"))).toBe(true);
    expect(isDisabled(buttonWithText(renderModel("error"), "Switch"))).toBe(true);
  });

  it.each(["downloading", "verifying", "extracting"] satisfies ModelStatus[])(
    "prevents duplicate downloads while %s is active",
    (status) => {
      const markup = renderModel(status);

      expect(isDisabled(buttonTag(markup, "download"))).toBe(true);
      expect(isDisabled(buttonTag(markup, "shield-check"))).toBe(true);
      expect(isDisabled(buttonTag(markup, "circle-x"))).toBe(false);
      expect(isDisabled(buttonTag(markup, "trash-2"))).toBe(true);
    },
  );

  it("routes failed downloads through retry instead of the primary download action", () => {
    const markup = renderModel("error", { download_error: "network unavailable" });

    expect(isDisabled(buttonTag(markup, "download"))).toBe(true);
    expect(isDisabled(buttonTag(markup, "rotate-cw"))).toBe(false);
  });

  it("does not offer retry for failed models without a download URL", () => {
    const markup = renderModel("error", {
      download_url: null,
      download_error: "manual model path is missing",
    });

    expect(isDisabled(buttonTag(markup, "download"))).toBe(true);
    expect(isDisabled(buttonTag(markup, "rotate-cw"))).toBe(true);
  });

  it("prevents deleting a model reserved for Dictation", () => {
    const dictationModel = mockModels()[0].id;
    const markup = renderModel("ready", {}, "other-model", dictationModel);

    expect(isDisabled(buttonTag(markup, "trash-2"))).toBe(true);
    expect(titleOf(buttonTag(markup, "trash-2"))).toBe(
      "Cannot delete a model used by transcription or Dictation",
    );
  });

  it.each(["ready", "installed", "unloaded"] satisfies ModelStatus[])(
    "does not offer download for usable %s models",
    (status) => {
      const markup = renderModel(status);

      expect(isDisabled(buttonTag(markup, "download"))).toBe(true);
    },
  );

  it("offers download only for downloadable missing models", () => {
    const missingMarkup = renderModel("missing");

    expect(isDisabled(buttonTag(missingMarkup, "download"))).toBe(false);
    expect(isDisabled(buttonTag(missingMarkup, "shield-check"))).toBe(false);
    expect(isDisabled(buttonTag(missingMarkup, "trash-2"))).toBe(true);
    expect(
      isDisabled(buttonTag(renderModel("missing", { download_url: null }), "download")),
    ).toBe(true);
  });

  it("does not allow deleting the active selected model", () => {
    const activeModelId = mockModels()[0].id;
    const markup = renderModel("ready", {}, activeModelId);

    expect(isDisabled(buttonTag(markup, "trash-2"))).toBe(true);
  });

  it.each(["ready", "installed", "unloaded", "error"] satisfies ModelStatus[])(
    "allows verify and delete for non-active %s models",
    (status) => {
      const markup = renderModel(status);

      expect(isDisabled(buttonTag(markup, "shield-check"))).toBe(false);
      expect(isDisabled(buttonTag(markup, "trash-2"))).toBe(false);
    },
  );

  it.each([
    ["openrouter-stt", "Requires OpenRouter API key"],
    ["openai-stt", "Requires OpenAI API key"],
    ["openai-realtime", "Requires OpenAI API key"],
  ])("keeps %s cloud models selectable without local file actions", (providerRuntime, label) => {
    const markup = renderModel("ready", {
      provider_runtime: providerRuntime,
      offline: false,
      download_url: null,
      size_mb: 0,
    });

    expect(markup).toContain(">API<");
    expect(markup).toContain(label);
    expect(isDisabled(buttonWithText(markup, "Switch"))).toBe(false);
    expect(markup).not.toContain("lucide-download");
    expect(markup).not.toContain("lucide-shield-check");
    expect(markup).not.toContain("lucide-trash-2");
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
  it("downloads sherpa-onnx and whisper-rs models that carry a URL", () => {
    expect(
      modelAcquireAction({
        provider_runtime: "sherpa-onnx",
        download_url: "https://example.invalid/model.tar.bz2",
        status: "missing",
      }),
    ).toMatchObject({ kind: "download", enabled: true });
    expect(
      modelAcquireAction({
        provider_runtime: "whisper-rs",
        download_url: "https://example.invalid/m.bin",
        status: "missing",
      }),
    ).toMatchObject({ kind: "download", enabled: true });
  });

  it("blocks download for models without a URL", () => {
    expect(
      modelAcquireAction({ provider_runtime: "whisper-rs", download_url: null, status: "missing" }),
    ).toMatchObject({ kind: "download", enabled: false, reason: "No download URL available" });
  });

  it("offers setup for built-in Qwen3-ASR models", () => {
    expect(
      modelAcquireAction({
        provider_runtime: "qwen3-asr",
        download_url: null,
        status: "missing",
      }),
    ).toMatchObject({
      kind: "download",
      enabled: true,
      label: "Set up Qwen3-ASR",
      reason: null,
    });
  });
});

describe("formatModelSize", () => {
  it("renders sizes below 1 GiB as MB", () => {
    expect(formatModelSize(75)).toBe("75 MB");
    expect(formatModelSize(465)).toBe("465 MB");
    expect(formatModelSize(1023)).toBe("1023 MB");
  });

  it("renders sizes at or above 1 GiB as GB with one decimal", () => {
    expect(formatModelSize(1024)).toBe("1.0 GB");
    expect(formatModelSize(1031)).toBe("1.0 GB");
    expect(formatModelSize(1465)).toBe("1.4 GB");
    expect(formatModelSize(1550)).toBe("1.5 GB");
  });

  it("renders invalid sizes as an em-dash placeholder", () => {
    expect(formatModelSize(Number.NaN)).toBe("—");
    expect(formatModelSize(-1)).toBe("—");
  });

  it("is rendered into the model row metadata strip", () => {
    const markup = renderModel("ready", { size_mb: 1465 });

    expect(markup).toContain("1.4 GB");
    expect(markup).not.toContain("1465 MB");
  });
});

describe("model download error rendering", () => {
  it("wraps download_error in a danger-toned span when present", () => {
    const markup = renderModel("error", { download_error: "network unavailable" });

    expect(markup).toContain('<span data-tone="danger">network unavailable</span>');
  });

  it("omits the toned span when download_error is null", () => {
    const markup = renderModel("ready", { download_error: null });

    expect(markup).not.toContain('data-tone="danger"');
  });
});

describe("model status badge text", () => {
  it.each([
    ["ready", "Ready"],
    ["installed", "Installed"],
    ["missing", "Missing"],
    ["downloading", "Downloading"],
    ["verifying", "Verifying"],
    ["extracting", "Extracting"],
    ["unloaded", "Unloaded"],
    ["error", "Error"],
  ] satisfies [ModelStatus, string][])(
    "renders %s as Title-Case '%s' in the badge",
    (status, label) => {
      const markup = renderModel(status);

      expect(markup).toMatch(
        new RegExp(`<span [^>]*data-tone="[^"]+">${label}</span>`),
      );
      expect(markup).not.toMatch(
        new RegExp(`<span [^>]*data-tone="[^"]+">${status}</span>`),
      );
    },
  );
});

describe("formatLanguageList", () => {
  it("maps known language codes to human-readable labels", () => {
    expect(formatLanguageList(["ko", "en", "multi"])).toBe(
      "Korean, English, Multilingual",
    );
    expect(formatLanguageList(["ja", "zh"])).toBe("Japanese, Chinese");
  });

  it("preserves unknown codes as-is", () => {
    expect(formatLanguageList(["xx", "en"])).toBe("xx, English");
  });

  it("deduplicates labels so codes that map to the same name are not repeated", () => {
    expect(formatLanguageList(["en", "en"])).toBe("English");
  });

  it("renders friendly labels in the model row", () => {
    const markup = renderModel("ready", { languages: ["ko", "en", "multi"] });

    expect(markup).toContain("Korean, English, Multilingual");
    expect(markup).not.toContain(">ko, en, multi<");
  });
});

describe("model switch disabled reason", () => {
  it.each([
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
  ] satisfies Array<[ModelStatus, boolean, string | null]>)(
    "describes switch availability for %s (selected=%s)",
    (status, isSelected, reason) => {
      expect(
        modelSwitchDisabledReason({ status, provider_runtime: "whisper-rs" }, isSelected),
      ).toBe(reason);
    },
  );
});

describe("model download disabled reason", () => {
  it("returns null when a missing model with a download URL is downloadable", () => {
    expect(
      modelDownloadDisabledReason({
        status: "missing",
        download_url: "https://example.invalid/m.bin",
      }),
    ).toBeNull();
  });

  it.each(["downloading", "verifying", "extracting"] satisfies ModelStatus[])(
    "flags %s as already in progress",
    (status) => {
      expect(
        modelDownloadDisabledReason({ status, download_url: "https://x" }),
      ).toBe("Download already in progress");
    },
  );

  it.each(["ready", "installed", "unloaded"] satisfies ModelStatus[])(
    "flags %s as already installed",
    (status) => {
      expect(
        modelDownloadDisabledReason({ status, download_url: "https://x" }),
      ).toBe("Model is already installed");
    },
  );

  it("flags error status with download URL as needing Retry", () => {
    expect(
      modelDownloadDisabledReason({ status: "error", download_url: "https://x" }),
    ).toBe("Download failed — use Retry");
  });

  it("flags missing-status models without a download URL", () => {
    expect(
      modelDownloadDisabledReason({ status: "missing", download_url: null }),
    ).toBe("No download URL available");
  });

  it("flags error-status models without a download URL as missing URL (most actionable)", () => {
    expect(
      modelDownloadDisabledReason({ status: "error", download_url: null }),
    ).toBe("No download URL available");
  });
});

describe("model verify disabled reason", () => {
  it.each(["downloading", "verifying", "extracting"] satisfies ModelStatus[])(
    "reports %s as in progress",
    (status) => {
      expect(modelVerifyDisabledReason({ status })).toBe("Download in progress");
    },
  );

  it.each(["ready", "installed", "unloaded", "missing", "error"] satisfies ModelStatus[])(
    "returns null for non-active-download status %s",
    (status) => {
      expect(modelVerifyDisabledReason({ status })).toBeNull();
    },
  );
});

describe("model retry disabled reason", () => {
  it("returns null when retry is available", () => {
    expect(
      modelRetryDisabledReason({ status: "error", download_url: "https://x" }),
    ).toBeNull();
  });

  it.each(["downloading", "verifying", "extracting"] satisfies ModelStatus[])(
    "flags %s as already in progress",
    (status) => {
      expect(modelRetryDisabledReason({ status, download_url: "https://x" })).toBe(
        "Download already in progress",
      );
    },
  );

  it("flags error-status models without a download URL", () => {
    expect(
      modelRetryDisabledReason({ status: "error", download_url: null }),
    ).toBe("No download URL available");
  });

  it.each(["ready", "installed", "unloaded", "missing"] satisfies ModelStatus[])(
    "reports nothing-to-retry for %s",
    (status) => {
      expect(
        modelRetryDisabledReason({ status, download_url: "https://x" }),
      ).toBe("Nothing to retry");
    },
  );
});

describe("model cancel download disabled reason", () => {
  it.each(["downloading", "verifying", "extracting"] satisfies ModelStatus[])(
    "returns null while %s is active",
    (status) => {
      expect(modelCancelDownloadDisabledReason({ status })).toBeNull();
    },
  );

  it.each(["ready", "installed", "unloaded", "missing", "error"] satisfies ModelStatus[])(
    "reports no active download for %s",
    (status) => {
      expect(modelCancelDownloadDisabledReason({ status })).toBe(
        "No active download",
      );
    },
  );
});

describe("model delete disabled reason", () => {
  it.each(["ready", "installed", "unloaded", "error"] satisfies ModelStatus[])(
    "blocks deletion of the active selected %s model",
    (status) => {
      expect(modelDeleteDisabledReason({ status }, true)).toBe(
        "Cannot delete a model used by transcription or Dictation",
      );
    },
  );

  it.each(["downloading", "verifying", "extracting"] satisfies ModelStatus[])(
    "blocks deletion of %s models when not selected",
    (status) => {
      expect(modelDeleteDisabledReason({ status }, false)).toBe(
        "Download in progress",
      );
    },
  );

  it("blocks deletion of missing models", () => {
    expect(modelDeleteDisabledReason({ status: "missing" }, false)).toBe(
      "Model is not downloaded",
    );
  });

  it.each(["ready", "installed", "unloaded", "error"] satisfies ModelStatus[])(
    "permits deletion of non-active %s models",
    (status) => {
      expect(modelDeleteDisabledReason({ status }, false)).toBeNull();
    },
  );
});

describe("disabled-reason titles render on the action buttons", () => {
  it("surfaces the download/verify/cancel/delete reasons while downloading", () => {
    const markup = renderModel("downloading");

    expect(titleOf(buttonTag(markup, "download"))).toBe("Download already in progress");
    expect(titleOf(buttonTag(markup, "shield-check"))).toBe("Download in progress");
    expect(titleOf(buttonTag(markup, "circle-x"))).toBe("Cancel Download");
    expect(titleOf(buttonTag(markup, "trash-2"))).toBe("Download in progress");
    expect(titleOf(buttonTag(markup, "rotate-cw"))).toBe("Download already in progress");
  });

  it("surfaces the switch reason on a missing model and routes failures to Retry", () => {
    const markup = renderModel("error", { download_url: "https://example.invalid/m.bin" });
    expect(titleOf(buttonWithText(markup, "Switch"))).toBe("Model has a download error");
    expect(titleOf(buttonTag(markup, "download"))).toBe("Download failed — use Retry");
    expect(titleOf(buttonTag(markup, "rotate-cw"))).toBe("Retry");
  });

  it("flags missing-without-url for both Download and Retry", () => {
    const markup = renderModel("missing", { download_url: null });
    expect(titleOf(buttonTag(markup, "download"))).toBe("No download URL available");
    expect(titleOf(buttonTag(markup, "rotate-cw"))).toBe("Nothing to retry");
    expect(titleOf(buttonTag(markup, "trash-2"))).toBe("Model is not downloaded");
  });

  it("explains why the active model cannot be deleted", () => {
    const activeId = mockModels()[0].id;
    const markup = renderModel("ready", {}, activeId);
    expect(titleOf(buttonTag(markup, "trash-2"))).toBe(
      "Cannot delete a model used by transcription or Dictation",
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
