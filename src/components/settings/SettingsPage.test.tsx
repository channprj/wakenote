import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import type { AppSnapshot } from "@/lib/types";
import type { SettingsSection } from "@/lib/navigation";
import { SettingsPage } from "./SettingsPage";
import { SettingsGrid } from "./settings-controls";
import type { SettingsActions } from "./types";

const actions: SettingsActions = {
  onPatch: vi.fn(),
  onSuspendDictationShortcut: vi.fn(),
  onResumeDictationShortcut: vi.fn(),
  onPressedModifierShortcut: vi.fn().mockResolvedValue(null),
  onChooseSaveRoot: vi.fn(),
  onRevealSaveFolder: vi.fn(),
  onChooseModelDirectory: vi.fn(),
  onRequestAccessibilityPermission: vi.fn(),
  onRequestMicrophonePermission: vi.fn(),
  onRequestScreenRecordingPermission: vi.fn(),
  onVerifyModel: vi.fn(),
  onDownloadModel: vi.fn(),
  onCancelModelDownload: vi.fn(),
  onDeleteModel: vi.fn(),
  onSaveOpenRouterApiKey: vi.fn(),
  onDeleteOpenRouterApiKey: vi.fn(),
};

function renderSection(section: SettingsSection, snapshot: AppSnapshot = mockSnapshot()) {
  return renderToStaticMarkup(
    <SettingsPage
      section={section}
      onSectionChange={() => {}}
      snapshot={snapshot}
      actions={actions}
    />,
  );
}

describe("SettingsPage ownership", () => {
  it("keeps application and window behavior in General", () => {
    const markup = renderSection("general");

    expect(markup).toContain("Launch at login");
    expect(markup).toContain("Show Dock icon");
    expect(markup).not.toContain("Threshold");
  });

  it("keeps input, permission, and timing controls in Audio", () => {
    const snapshot = mockSnapshot();
    snapshot.permissions.accessibility = {
      status: "denied",
      label: "Needs access",
      detail: "WakeNote needs Accessibility access to type Dictation results at the cursor.",
      can_request: true,
      can_open_settings: true,
    };
    const markup = renderSection("audio", snapshot);

    expect(markup).toContain("Microphone Permission");
    expect(markup).toContain("Accessibility Permission");
    expect(markup).toContain(
      "WakeNote needs Accessibility access to type Dictation results",
    );
    expect(markup).toContain('aria-label="Primary microphone"');
    expect(markup).toContain('aria-label="Secondary microphone"');
    expect(markup).toContain("1 / 2 selected");
    expect(markup).toContain("Merge microphone inputs");
    expect(markup).toContain("Input monitoring uses Primary only.");
    expect(markup).toContain("Threshold");
    expect(markup).not.toContain("OpenRouter");
  });

  it("keeps model files and storage paths separate", () => {
    expect(renderSection("models")).toContain("Model Directory");
    expect(renderSection("storage")).toContain("Save Root");
  });

  it("keeps shortcut dictation in its own settings section", () => {
    const markup = renderSection("dictation");

    expect(markup).toContain("Shortcut dictation");
    expect(markup).toContain("Enable shortcut dictation");
    expect(markup).toContain("Dictation shortcut");
    expect(markup).toContain("Dictation language");
    expect(markup).toContain("Dictation model");
    expect(markup).toContain("Copy result to clipboard");
    expect(markup).toContain("Remove trailing space");
    expect(markup).toContain("Start sound");
    expect(markup).toContain("Stop sound");
    expect(markup).toContain("Cue volume");
    expect(markup).toContain("Bubble position");
    expect(markup).toContain("Auto-detect");
    expect(markup).toContain(
      "Hold the shortcut to record. Release it to transcribe and type at the cursor.",
    );
    expect(markup).not.toContain("Press once to record");
  });

  it("keeps external behavior in Integrations and expert controls in Advanced", () => {
    const integrations = renderSection("integrations");
    const advanced = renderSection("advanced");

    expect(integrations).toContain("OpenRouter");
    expect(integrations).toContain("Floating overlay");
    expect(advanced).toContain("VAD gate");
    expect(advanced).not.toContain("OpenRouter");
  });

  it("renders seven compact settings tabs without Library", () => {
    const markup = renderSection("general");

    for (const label of [
      "General",
      "Audio",
      "Dictation",
      "Models",
      "Storage",
      "Integrations",
      "Advanced",
    ]) {
      expect(markup).toContain(label);
    }
    expect(markup).not.toContain("Library");
  });
});

describe("SettingsPage balanced masonry", () => {
  it.each([
    ["general", "2"],
    ["advanced", "2"],
    ["storage", "3"],
    ["integrations", "3"],
    ["audio", "3"],
    ["dictation", "2"],
  ] as const)(
    "caps the %s grid by its rendered card count",
    (section, maxColumns) => {
      expect(renderSection(section)).toContain(
        `data-max-columns="${maxColumns}"`,
      );
    },
  );

  it("places Model files before a separate model-card grid", () => {
    const markup = renderSection("models");
    const files = markup.indexOf('data-slot="model-files-card"');
    const models = markup.indexOf('data-slot="model-card-grid"');

    expect(markup).toContain('class="models-settings"');
    expect(files).toBeGreaterThan(-1);
    expect(models).toBeGreaterThan(files);
  });

  it("caps a three-child Settings grid at two columns", () => {
    const markup = renderToStaticMarkup(
      <SettingsGrid maxColumns={2}>
        <div>One</div>
        <div>Two</div>
        <div>Three</div>
      </SettingsGrid>,
    );

    expect(markup).toContain('data-max-columns="2"');
  });

  it("keeps the one-column fallback for a single child", () => {
    const markup = renderToStaticMarkup(
      <SettingsGrid maxColumns={3}>
        <div>Only</div>
      </SettingsGrid>,
    );

    expect(markup).toContain('data-max-columns="1"');
  });
});
