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
  it("normalizes valid shortcuts and rejects invalid ones", () => {
    const accepted: Array<[string, string]> = [
      // Aliases, whitespace, case, and modifier order.
      [" Command + SHIFT + Option + Control + D ", "ctrl+alt+shift+cmd+d"],
      // Modifier chords, modified keys, physical modifiers, and single keys.
      [" Shift + Control ", "ctrl+shift"],
      ["Control+V", "ctrl+v"],
      ["Option+Command", "alt+cmd"],
      ["Shift+Z", "shift+z"],
      ["Z", "z"],
      ["Space", "space"],
      ["LeftControl", "leftctrl"],
    ];

    for (const [input, canonical] of accepted) {
      expect(normalizeDictationShortcut(input), input).toBe(canonical);
    }

    for (const shortcut of [
      "",
      "ctrl",
      "fn+space",
      "ctrl+a+b",
      "ctrl++d",
      "ctrl+ctrl+d",
    ]) {
      expect(() => normalizeDictationShortcut(shortcut), shortcut).toThrow();
    }
  });

  it("derives shortcuts from physical key events", () => {
    const cases: Array<[string, ReturnType<typeof keyEvent>, string | null]> = [
      // The physical key code keeps Option-modified characters stable.
      ["Option+D as ∂", keyEvent("KeyD", { key: "∂", altKey: true }), "alt+d"],
      ["F8", keyEvent("F8"), "f8"],
      // A physical modifier key on its own.
      ["AltLeft", keyEvent("AltLeft", { key: "Alt", altKey: true }), "leftalt"],
      // Unsupported keys are rejected.
      ["MediaPlayPause", keyEvent("MediaPlayPause", { altKey: true }), null],
      // A modifier-only chord once the second modifier is pressed.
      [
        "ShiftLeft with Control held",
        keyEvent("ShiftLeft", {
          key: "Shift",
          ctrlKey: true,
          shiftKey: true,
        }),
        "ctrl+shift",
      ],
    ];

    for (const [label, event, expected] of cases) {
      expect(dictationShortcutFromKeyboardEvent(event), label).toBe(expected);
    }
  });

  it("formats canonical shortcuts for people", () => {
    expect(formatDictationShortcut("ctrl+alt+shift+cmd+d")).toBe(
      "Control + Option + Shift + Command + D",
    );
    expect(formatDictationShortcut("alt+space")).toBe("Option + Space");
    expect(formatDictationShortcut("ctrl+shift")).toBe("Control + Shift");
    expect(formatDictationShortcut("leftctrl")).toBe("Left Control");
  });

  it("emits only canonical shortcuts accepted by its own validator", () => {
    for (const code of SUPPORTED_DICTATION_KEY_CODES) {
      const event = keyEvent(code);
      const shortcut = dictationShortcutFromKeyboardEvent(event);
      expect(shortcut, code).not.toBeNull();
      expect(normalizeDictationShortcut(shortcut ?? ""), code).toBe(shortcut);
    }
  });
});
