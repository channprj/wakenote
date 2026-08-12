// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import { CustomSourceSettings } from "./CustomSourceSettings";

afterEach(cleanup);

describe("CustomSourceSettings", () => {
  it("preserves an in-progress edit across equivalent settings refreshes", () => {
    const onPatch = vi.fn();
    const settings = {
      ...defaultSettings(),
      custom_sources: [
        {
          id: "custom-source-1",
          label: "Google Meet",
          title_patterns: ["Google Meet", "Meet"],
          auto_prompt: true,
        },
      ],
    };
    const { rerender } = render(
      <CustomSourceSettings settings={settings} onPatch={onPatch} />,
    );
    const input = screen.getByRole("textbox", {
      name: "Custom source",
    }) as HTMLInputElement;

    fireEvent.change(input, {
      target: { value: "Product Call, Weekly Sync" },
    });
    rerender(
      <CustomSourceSettings
        settings={{
          ...settings,
          custom_sources: settings.custom_sources.map((source) => ({
            ...source,
          })),
        }}
        onPatch={onPatch}
      />,
    );

    expect(input.value).toBe("Product Call, Weekly Sync");
    fireEvent.blur(input);
    expect(onPatch).toHaveBeenCalledWith({
      custom_sources: [
        {
          id: "custom-source-1",
          label: "Product Call",
          title_patterns: ["Product Call", "Weekly Sync"],
          auto_prompt: true,
        },
      ],
    });
  });

  it("accepts a real external custom-source change", () => {
    const settings = {
      ...defaultSettings(),
      custom_sources: [
        {
          id: "custom-source-1",
          label: "Google Meet",
          title_patterns: ["Google Meet"],
          auto_prompt: true,
        },
      ],
    };
    const { rerender } = render(
      <CustomSourceSettings settings={settings} onPatch={vi.fn()} />,
    );

    rerender(
      <CustomSourceSettings
        settings={{
          ...settings,
          custom_sources: [
            {
              ...settings.custom_sources[0],
              label: "Zoom",
              title_patterns: ["Zoom", "Zoom Meeting"],
            },
          ],
        }}
        onPatch={vi.fn()}
      />,
    );

    expect(
      (screen.getByRole("textbox", {
        name: "Custom source",
      }) as HTMLInputElement).value,
    ).toBe("Zoom, Zoom Meeting");
  });
});
