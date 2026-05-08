import { isValidElement, type ReactElement, type ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "../lib/app-state";
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
});
