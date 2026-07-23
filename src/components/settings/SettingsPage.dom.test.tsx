// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import { SettingsPage } from "./SettingsPage";
import type { SettingsActions } from "./types";

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

globalThis.ResizeObserver = TestResizeObserver as typeof ResizeObserver;

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});

function makeActions(): SettingsActions {
  return {
    onPatch: vi.fn(),
    onChooseSaveRoot: vi.fn(),
    onRevealSaveFolder: vi.fn(),
    onChooseModelDirectory: vi.fn(),
    onRequestMicrophonePermission: vi.fn(),
    onRequestScreenRecordingPermission: vi.fn(),
    onVerifyModel: vi.fn(),
    onDownloadModel: vi.fn(),
    onCancelModelDownload: vi.fn(),
    onDeleteModel: vi.fn(),
    onSaveOpenRouterApiKey: vi.fn(),
    onDeleteOpenRouterApiKey: vi.fn(),
  };
}

describe("SettingsPage interactions", () => {
  it("keeps a controlled active tab visible inside the compact tab scroller", () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoView,
    });

    render(
      <SettingsPage
        section="advanced"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={makeActions()}
      />,
    );

    expect(scrollIntoView).toHaveBeenCalledWith({
      block: "nearest",
      inline: "nearest",
    });
  });

  it("reports settings tab changes through the controlled section contract", async () => {
    const onSectionChange = vi.fn();
    render(
      <SettingsPage
        section="general"
        onSectionChange={onSectionChange}
        snapshot={mockSnapshot()}
        actions={makeActions()}
      />,
    );

    await userEvent.click(screen.getByRole("tab", { name: "Audio" }));
    expect(onSectionChange).toHaveBeenCalledWith("audio");
  });

  it("preserves the exact General patch key", async () => {
    const actions = makeActions();
    render(
      <SettingsPage
        section="general"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={actions}
      />,
    );

    await userEvent.click(screen.getByRole("switch", { name: "Launch at login" }));
    expect(actions.onPatch).toHaveBeenCalledWith({ launch_at_login: true });
  });

  it("preserves the Save Root patch key", () => {
    const actions = makeActions();
    render(
      <SettingsPage
        section="storage"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={actions}
      />,
    );

    fireEvent.change(screen.getByLabelText("Save Root"), { target: { value: "/tmp/notes" } });
    expect(actions.onPatch).toHaveBeenCalledWith({ save_root: "/tmp/notes" });
  });

  it("preserves the auto-type integration patch key", async () => {
    const actions = makeActions();
    render(
      <SettingsPage
        section="integrations"
        onSectionChange={() => {}}
        snapshot={mockSnapshot()}
        actions={actions}
      />,
    );

    await userEvent.click(
      screen.getByRole("switch", { name: "Auto-type transcripts into cursor" }),
    );
    expect(actions.onPatch).toHaveBeenCalledWith({ auto_transcript_input_enabled: true });
  });

  it("disables the floating overlay position when the overlay is hidden", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.show_floating_overlay = false;

    render(
      <SettingsPage
        section="integrations"
        onSectionChange={() => {}}
        snapshot={snapshot}
        actions={makeActions()}
      />,
    );

    const position = screen.getByRole("combobox", { name: "Floating overlay position" });
    expect((position as HTMLButtonElement).disabled).toBe(true);
  });

  it("keeps later Audio controls mounted when system sources are removed", () => {
    const actions = makeActions();
    const enabled = mockSnapshot();
    enabled.settings.system_audio_enabled = true;
    const { rerender } = render(
      <SettingsPage
        section="audio"
        onSectionChange={() => {}}
        snapshot={enabled}
        actions={actions}
      />,
    );

    expect(screen.getByText("Recognized system sources")).toBeTruthy();
    expect(screen.getByText("Chunk timing")).toBeTruthy();

    const disabled = mockSnapshot();
    disabled.settings.system_audio_enabled = false;
    rerender(
      <SettingsPage
        section="audio"
        onSectionChange={() => {}}
        snapshot={disabled}
        actions={actions}
      />,
    );

    expect(screen.queryByText("Recognized system sources")).toBeNull();
    expect(screen.getByText("Chunk timing")).toBeTruthy();
  });
});
