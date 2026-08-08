import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import { AudioSettings } from "./AudioSettings";
import type { SettingsActions } from "./types";

const actions: SettingsActions = {
  onPatch: vi.fn(),
  onPreviewSubtitle: vi.fn(),
  onSetMicrophoneInputVolume: vi.fn(),
  onSuspendDictationShortcut: vi.fn(),
  onResumeDictationShortcut: vi.fn(),
  onPressedModifierShortcut: vi.fn().mockResolvedValue(null),
  onChooseSaveRoot: vi.fn(),
  onRevealSaveFolder: vi.fn(),
  onChooseModelDirectory: vi.fn(),
  onOpenDictionaryFile: vi.fn(),
  onReloadDictionaryFile: vi.fn(),
  onRequestAccessibilityPermission: vi.fn(),
  onRequestMicrophonePermission: vi.fn(),
  onRequestScreenRecordingPermission: vi.fn(),
  onVerifyModel: vi.fn(),
  onDownloadModel: vi.fn(),
  onCancelModelDownload: vi.fn(),
  onDeleteModel: vi.fn(),
  onSaveOpenRouterApiKey: vi.fn(),
  onDeleteOpenRouterApiKey: vi.fn(),
  onSaveOpenAiApiKey: vi.fn(),
  onDeleteOpenAiApiKey: vi.fn(),
  onSaveSonioxApiKey: vi.fn(),
  onDeleteSonioxApiKey: vi.fn(),
};

describe("AudioSettings", () => {
  it("offers the System Settings drag flow for app-list permissions", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.system_audio_enabled = true;
    snapshot.permissions.accessibility.status = "not_determined";
    snapshot.permissions.accessibility.can_request = true;
    snapshot.permissions.accessibility.can_open_settings = true;
    snapshot.permissions.screen_recording.status = "not_determined";
    snapshot.permissions.screen_recording.can_request = false;
    snapshot.permissions.screen_recording.can_open_settings = true;

    const markup = renderToStaticMarkup(
      <AudioSettings snapshot={snapshot} actions={actions} />,
    );

    expect(markup).not.toContain("Allow Accessibility");
    expect(markup).toContain("Open System Settings");
  });

  it("keeps microphone authorization on its native request action", () => {
    const snapshot = mockSnapshot();
    snapshot.permissions.microphone.status = "not_determined";
    snapshot.permissions.microphone.can_request = true;

    const markup = renderToStaticMarkup(
      <AudioSettings snapshot={snapshot} actions={actions} />,
    );

    expect(markup).toContain("Allow Microphone");
  });

  it("keeps configured microphones visible during a transient enumeration fallback", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.selected_microphone = "input-primary";
    snapshot.settings.selected_microphone_label = "Studio Mic";
    snapshot.settings.capture_microphones = [
      { id: "input-primary", label: "Studio Mic" },
      { id: "input-secondary", label: "Desk Mic" },
    ];
    snapshot.microphones = [
      {
        id: "default",
        label: "System Default",
        available: true,
        fallback: true,
      },
    ];

    const markup = renderToStaticMarkup(
      <AudioSettings snapshot={snapshot} actions={actions} />,
    );

    expect(markup).toContain("Studio Mic (Unavailable)");
    expect(markup).toContain("Desk Mic (Unavailable)");
    expect(markup).toContain('aria-label="Primary microphone"');
    expect(markup).toContain('aria-label="Secondary microphone"');
  });
});
