import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { defaultSettings, mockModels } from "../lib/app-state";
import type { ModelDescriptor, ModelStatus } from "../lib/types";
import { ModelManager, formatLanguageList, formatModelSize } from "./ModelManager";

function renderModel(
  status: ModelStatus,
  overrides: Partial<ModelDescriptor> = {},
  selectedModel = "other-model",
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
      settings={{ ...defaultSettings(), selected_model: selectedModel }}
      onPatch={() => {}}
      onVerify={() => {}}
      onDownload={() => {}}
      onCancelDownload={() => {}}
      onDelete={() => {}}
    />,
  );
}

function buttonTag(markup: string, title: string) {
  const match = markup.match(new RegExp(`<button[^>]*title="${title}"[^>]*>`));
  expect(match, `expected ${title} button`).not.toBeNull();
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

      expect(isDisabled(buttonTag(markup, "Download"))).toBe(true);
      expect(isDisabled(buttonTag(markup, "Verify"))).toBe(true);
      expect(isDisabled(buttonTag(markup, "Cancel Download"))).toBe(false);
      expect(isDisabled(buttonTag(markup, "Delete"))).toBe(true);
    },
  );

  it("routes failed downloads through retry instead of the primary download action", () => {
    const markup = renderModel("error", { download_error: "network unavailable" });

    expect(isDisabled(buttonTag(markup, "Download"))).toBe(true);
    expect(isDisabled(buttonTag(markup, "Retry"))).toBe(false);
  });

  it("does not offer retry for failed models without a download URL", () => {
    const markup = renderModel("error", {
      download_url: null,
      download_error: "manual model path is missing",
    });

    expect(isDisabled(buttonTag(markup, "Download"))).toBe(true);
    expect(isDisabled(buttonTag(markup, "Retry"))).toBe(true);
  });

  it.each(["ready", "installed", "unloaded"] satisfies ModelStatus[])(
    "does not offer download for usable %s models",
    (status) => {
      const markup = renderModel(status);

      expect(isDisabled(buttonTag(markup, "Download"))).toBe(true);
    },
  );

  it("offers download only for downloadable missing models", () => {
    const missingMarkup = renderModel("missing");

    expect(isDisabled(buttonTag(missingMarkup, "Download"))).toBe(false);
    expect(isDisabled(buttonTag(missingMarkup, "Verify"))).toBe(false);
    expect(isDisabled(buttonTag(missingMarkup, "Delete"))).toBe(true);
    expect(
      isDisabled(buttonTag(renderModel("missing", { download_url: null }), "Download")),
    ).toBe(true);
  });

  it("does not allow deleting the active selected model", () => {
    const activeModelId = mockModels()[0].id;
    const markup = renderModel("ready", {}, activeModelId);

    expect(isDisabled(buttonTag(markup, "Delete"))).toBe(true);
  });

  it.each(["ready", "installed", "unloaded", "error"] satisfies ModelStatus[])(
    "allows verify and delete for non-active %s models",
    (status) => {
      const markup = renderModel(status);

      expect(isDisabled(buttonTag(markup, "Verify"))).toBe(false);
      expect(isDisabled(buttonTag(markup, "Delete"))).toBe(false);
    },
  );
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
