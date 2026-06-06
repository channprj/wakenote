import { describe, expect, it } from "vitest";
import { shouldHandleFrontendHideShortcut } from "./window-shortcuts";

describe("window shortcuts", () => {
  it("delegates macOS Cmd+H to the native application menu", () => {
    expect(
      shouldHandleFrontendHideShortcut(
        {
          key: "h",
          metaKey: true,
          ctrlKey: false,
          shiftKey: false,
          altKey: false,
        },
        "MacIntel",
      ),
    ).toBe(false);
  });

  it("keeps the non-macOS Ctrl+H frontend fallback", () => {
    expect(
      shouldHandleFrontendHideShortcut(
        {
          key: "h",
          metaKey: false,
          ctrlKey: true,
          shiftKey: false,
          altKey: false,
        },
        "Win32",
      ),
    ).toBe(true);
  });
});
