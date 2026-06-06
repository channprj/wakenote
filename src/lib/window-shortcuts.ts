export type WindowShortcutKeyEvent = {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
};

export function shouldHandleFrontendHideShortcut(
  event: WindowShortcutKeyEvent,
  platform: string,
): boolean {
  const key = event.key.toLowerCase();
  if (key !== "h" || event.shiftKey || event.altKey) {
    return false;
  }

  if (platform.toLowerCase().includes("mac")) {
    return false;
  }

  return event.ctrlKey;
}
