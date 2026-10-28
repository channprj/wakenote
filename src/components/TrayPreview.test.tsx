import { isValidElement, type ReactElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "../lib/app-state";
import type { TrayState } from "../lib/types";
import { TrayPreview } from "./TrayPreview";

function collectButtons(node: ReactNode, buttons: ReactElement[] = []): ReactElement[] {
  if (Array.isArray(node)) {
    node.forEach((child) => collectButtons(child, buttons));
    return buttons;
  }

  if (!isValidElement(node)) {
    return buttons;
  }

  if (node.type === "button") {
    buttons.push(node);
  }

  const props = node.props as { children?: ReactNode };
  collectButtons(props.children, buttons);
  return buttons;
}

function buttonByText(node: ReactNode, text: string) {
  const button = collectButtons(node).find((candidate) =>
    renderToStaticMarkup(candidate).includes(text),
  );
  expect(button, `expected ${text} button`).toBeTruthy();
  return button as ReactElement<{ disabled?: boolean; onClick?: () => void }>;
}

describe("tray preview", () => {
  it("does not render inert tray action buttons in the settings surface", () => {
    const snapshot = mockSnapshot();
    const onPatch = vi.fn();
    const onRevealSaveFolder = vi.fn();
    const onCancelCurrentOperation = vi.fn();
    const preview = TrayPreview({
      settings: snapshot.settings,
      status: snapshot.status,
      onPatch,
      onRevealSaveFolder,
      onCancelCurrentOperation,
    });

    expect(buttonByText(preview, "Reveal Save Folder").props.onClick).toBe(onRevealSaveFolder);
    expect(buttonByText(preview, "Cancel Current Operation").props.onClick).toBe(
      onCancelCurrentOperation,
    );
    expect(buttonByText(preview, "Open Settings").props.disabled).toBe(true);
  });

  it("surfaces a why-disabled tooltip on the always-disabled Open Settings button", () => {
    const snapshot = mockSnapshot();
    const preview = TrayPreview({
      settings: snapshot.settings,
      status: snapshot.status,
      onPatch: vi.fn(),
      onRevealSaveFolder: vi.fn(),
      onCancelCurrentOperation: vi.fn(),
    });

    const openSettings = buttonByText(preview, "Open Settings") as ReactElement<{
      disabled?: boolean;
      title?: string;
    }>;
    expect(openSettings.props.title).toBe("Settings panel is already open");

    const revealMarkup = renderToStaticMarkup(buttonByText(preview, "Reveal Save Folder"));
    expect(revealMarkup).not.toContain("title=");
  });

  it("derives the tray-state Badge tone from trayStateBadgeTone for every state", () => {
    const cases: [TrayState, string][] = [
      ["idle", "neutral"],
      ["listening", "primary"],
      ["recording", "success"],
      ["transcribing", "warning"],
      ["paused", "warning"],
      ["error", "danger"],
    ];

    for (const [trayState, expectedTone] of cases) {
      const snapshot = mockSnapshot();
      const preview = TrayPreview({
        settings: snapshot.settings,
        status: { ...snapshot.status, tray_state: trayState },
        onPatch: vi.fn(),
        onRevealSaveFolder: vi.fn(),
        onCancelCurrentOperation: vi.fn(),
      });
      const markup = renderToStaticMarkup(preview);
      expect(markup, `tray_state=${trayState}`).toMatch(
        new RegExp(`<span [^>]*data-tone="${expectedTone}">[^<]*</span>`),
      );
    }
  });
});
