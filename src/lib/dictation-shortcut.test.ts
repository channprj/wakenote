import { describe, expect, it } from "vitest";
import {
  SUPPORTED_DICTATION_KEY_CODES,
  dictationShortcutFromKeyboardEvent,
  formatDictationShortcut,
  normalizeDictationShortcut,
} from "./dictation-shortcut";

function keyEvent(
  code: string,
  overrides: Partial<{
    key: string;
    ctrlKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
    metaKey: boolean;
  }> = {},
) {
  return {
    code,
    key: overrides.key ?? code,
    ctrlKey: overrides.ctrlKey ?? false,
    altKey: overrides.altKey ?? false,
    shiftKey: overrides.shiftKey ?? false,
    metaKey: overrides.metaKey ?? false,
  };
}

describe("dictation shortcut helpers", () => {
  it("normalizes aliases, whitespace, case, and modifier order", () => {
    expect(normalizeDictationShortcut(" Command + SHIFT + Option + Control + D ")).toBe(
      "ctrl+alt+shift+cmd+d",
    );
  });

  it("accepts two-modifier shortcuts and rejects unsafe combinations", () => {
    expect(normalizeDictationShortcut(" Shift + Control ")).toBe("ctrl+shift");

    for (const shortcut of [
      "",
      "ctrl",
      "space",
      "fn+space",
      "ctrl+a+b",
      "ctrl++d",
      "ctrl+ctrl+d",
    ]) {
      expect(() => normalizeDictationShortcut(shortcut), shortcut).toThrow();
    }
  });

  it("uses the physical key code so Option-modified characters stay stable", () => {
    expect(
      dictationShortcutFromKeyboardEvent(
        keyEvent("KeyD", { key: "∂", altKey: true }),
      ),
    ).toBe("alt+d");
    expect(dictationShortcutFromKeyboardEvent(keyEvent("F8"))).toBe("f8");
  });

  it("returns null while only a modifier or an unsupported key is pressed", () => {
    expect(
      dictationShortcutFromKeyboardEvent(
        keyEvent("AltLeft", { key: "Alt", altKey: true }),
      ),
    ).toBeNull();
    expect(
      dictationShortcutFromKeyboardEvent(
        keyEvent("MediaPlayPause", { altKey: true }),
      ),
    ).toBeNull();
  });

  it("captures a modifier-only chord when the second modifier is pressed", () => {
    expect(
      dictationShortcutFromKeyboardEvent(
        keyEvent("ShiftLeft", {
          key: "Shift",
          ctrlKey: true,
          shiftKey: true,
        }),
      ),
    ).toBe("ctrl+shift");
  });

  it("formats canonical shortcuts for people", () => {
    expect(formatDictationShortcut("ctrl+alt+shift+cmd+d")).toBe(
      "Control + Option + Shift + Command + D",
    );
    expect(formatDictationShortcut("alt+space")).toBe("Option + Space");
    expect(formatDictationShortcut("ctrl+shift")).toBe("Control + Shift");
  });

  it("emits only canonical shortcuts accepted by its own validator", () => {
    for (const code of SUPPORTED_DICTATION_KEY_CODES) {
      const event = code.startsWith("F")
        ? keyEvent(code)
        : keyEvent(code, { altKey: true });
      const shortcut = dictationShortcutFromKeyboardEvent(event);
      expect(shortcut, code).not.toBeNull();
      expect(normalizeDictationShortcut(shortcut ?? ""), code).toBe(shortcut);
    }
  });
});
