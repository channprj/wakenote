import { describe, expect, it } from "vitest";
import { shouldHandleFrontendHideShortcut } from "./window-shortcuts";

describe("window shortcuts", () => {
  it("handles the hide shortcut in the frontend only off macOS", () => {
    const cases: Array<[string, Parameters<typeof shouldHandleFrontendHideShortcut>, boolean]> = [
      // macOS Cmd+H is delegated to the native application menu.
      [
        "macOS Cmd+H",
        [
          {
            key: "h",
            metaKey: true,
            ctrlKey: false,
            shiftKey: false,
            altKey: false,
          },
          "MacIntel",
        ],
        false,
      ],
      // Non-macOS Ctrl+H keeps the frontend fallback.
      [
        "Windows Ctrl+H",
        [
          {
            key: "h",
            metaKey: false,
            ctrlKey: true,
            shiftKey: false,
            altKey: false,
          },
          "Win32",
        ],
        true,
      ],
    ];

    for (const [label, args, expected] of cases) {
      expect(shouldHandleFrontendHideShortcut(...args), label).toBe(expected);
    }
  });
});
