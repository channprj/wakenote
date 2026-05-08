import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { defaultSettings, mockModels } from "../lib/app-state";
import type { ModelDescriptor, ModelStatus } from "../lib/types";
import { ModelManager } from "./ModelManager";

function renderModel(status: ModelStatus, overrides: Partial<ModelDescriptor> = {}) {
  const model: ModelDescriptor = {
    ...mockModels()[0],
    status,
    download_url: "https://example.invalid/model.bin",
    ...overrides,
  };

  return renderToStaticMarkup(
    <ModelManager
      models={[model]}
      settings={{ ...defaultSettings(), selected_model: "other-model" }}
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

function isDisabled(button: string) {
  return /\sdisabled(=""|\s|>)/.test(button);
}

describe("model manager actions", () => {
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

  it.each(["ready", "installed", "unloaded", "error"] satisfies ModelStatus[])(
    "allows verify and delete for non-active %s models",
    (status) => {
      const markup = renderModel(status);

      expect(isDisabled(buttonTag(markup, "Verify"))).toBe(false);
      expect(isDisabled(buttonTag(markup, "Delete"))).toBe(false);
    },
  );
});
