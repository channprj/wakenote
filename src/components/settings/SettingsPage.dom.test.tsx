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

afterEach(cleanup);

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
});
