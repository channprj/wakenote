export interface DictationShortcutKeyEvent {
  code: string;
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}

const LETTER_KEY_CODES = Array.from(
  { length: 26 },
  (_, index) => `Key${String.fromCharCode(65 + index)}`,
);
const DIGIT_KEY_CODES = Array.from({ length: 10 }, (_, index) => `Digit${index}`);
const FUNCTION_KEY_CODES = Array.from({ length: 24 }, (_, index) => `F${index + 1}`);
const NAMED_KEY_CODES = [
  "Space",
  "Enter",
  "Tab",
  "Backspace",
  "Delete",
  "ArrowUp",
  "ArrowDown",
  "ArrowLeft",
  "ArrowRight",
  "Home",
  "End",
  "PageUp",
  "PageDown",
] as const;

export const SUPPORTED_DICTATION_KEY_CODES = [
  ...LETTER_KEY_CODES,
  ...DIGIT_KEY_CODES,
  ...FUNCTION_KEY_CODES,
  ...NAMED_KEY_CODES,
] as const;

const MODIFIER_ALIASES: Readonly<Record<string, "ctrl" | "alt" | "shift" | "cmd">> = {
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  cmd: "cmd",
  command: "cmd",
  meta: "cmd",
  super: "cmd",
};

const MODIFIER_ORDER = ["ctrl", "alt", "shift", "cmd"] as const;
const NAMED_MAIN_KEYS = new Set([
  "space",
  "enter",
  "tab",
  "backspace",
  "delete",
  "arrowup",
  "arrowdown",
  "arrowleft",
  "arrowright",
  "home",
  "end",
  "pageup",
  "pagedown",
]);

export function normalizeDictationShortcut(raw: string): string {
  const parts = raw.split("+").map((part) => part.trim().toLowerCase());
  if (parts.length === 0 || parts.some((part) => part.length === 0)) {
    throw new Error("Dictation shortcut cannot be empty");
  }
  if (parts.some((part) => part === "fn" || part === "function")) {
    throw new Error("The fn key is not supported for dictation shortcuts");
  }

  const modifiers = new Set<(typeof MODIFIER_ORDER)[number]>();
  const mainKeys: string[] = [];
  for (const part of parts) {
    const modifier = MODIFIER_ALIASES[part];
    if (modifier) {
      if (modifiers.has(modifier)) {
        throw new Error(`Duplicate shortcut modifier: ${modifier}`);
      }
      modifiers.add(modifier);
    } else {
      mainKeys.push(part);
    }
  }
  if (mainKeys.length !== 1) {
    throw new Error("Dictation shortcut must contain exactly one main key");
  }

  const mainKey = mainKeys[0] ?? "";
  const functionKey = /^f(?:[1-9]|1\d|2[0-4])$/.test(mainKey);
  const supportedMainKey =
    /^[a-z0-9]$/.test(mainKey) ||
    functionKey ||
    NAMED_MAIN_KEYS.has(mainKey);
  if (!supportedMainKey) {
    throw new Error(`Unsupported dictation shortcut key: ${mainKey}`);
  }
  if (modifiers.size === 0 && !functionKey) {
    throw new Error(
      "Dictation shortcut must include a modifier unless it uses an F-key",
    );
  }

  return [
    ...MODIFIER_ORDER.filter((modifier) => modifiers.has(modifier)),
    mainKey,
  ].join("+");
}

export function dictationShortcutFromKeyboardEvent(
  event: DictationShortcutKeyEvent,
): string | null {
  const mainKey = mainKeyFromCode(event.code);
  if (!mainKey) {
    return null;
  }
  const parts = [
    event.ctrlKey ? "ctrl" : null,
    event.altKey ? "alt" : null,
    event.shiftKey ? "shift" : null,
    event.metaKey ? "cmd" : null,
    mainKey,
  ].filter((part): part is string => Boolean(part));
  try {
    return normalizeDictationShortcut(parts.join("+"));
  } catch {
    return null;
  }
}

export function formatDictationShortcut(shortcut: string): string {
  const labels: Readonly<Record<string, string>> = {
    ctrl: "Control",
    alt: "Option",
    shift: "Shift",
    cmd: "Command",
    space: "Space",
    enter: "Enter",
    tab: "Tab",
    backspace: "Backspace",
    delete: "Delete",
    arrowup: "Arrow Up",
    arrowdown: "Arrow Down",
    arrowleft: "Arrow Left",
    arrowright: "Arrow Right",
    home: "Home",
    end: "End",
    pageup: "Page Up",
    pagedown: "Page Down",
  };
  return normalizeDictationShortcut(shortcut)
    .split("+")
    .map((part) => labels[part] ?? part.toUpperCase())
    .join(" + ");
}

function mainKeyFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) {
    return code.slice(3).toLowerCase();
  }
  if (/^Digit\d$/.test(code)) {
    return code.slice(5);
  }
  if (/^F(?:[1-9]|1\d|2[0-4])$/.test(code)) {
    return code.toLowerCase();
  }
  const named: Readonly<Record<string, string>> = {
    Space: "space",
    Enter: "enter",
    Tab: "tab",
    Backspace: "backspace",
    Delete: "delete",
    ArrowUp: "arrowup",
    ArrowDown: "arrowdown",
    ArrowLeft: "arrowleft",
    ArrowRight: "arrowright",
    Home: "home",
    End: "end",
    PageUp: "pageup",
    PageDown: "pagedown",
  };
  return named[code] ?? null;
}
