import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "../lib/app-state";
import { SettingsPanel } from "./SettingsPanel";
import type { AppSnapshot } from "../lib/types";

function renderSettingsPanel(snapshot: AppSnapshot) {
  return renderToStaticMarkup(
    <SettingsPanel
      activeSection="general"
      snapshot={snapshot}
      onPatch={() => {}}
      onRefresh={() => {}}
      onStartLiveCapture={() => {}}
      onStopLiveCapture={() => {}}
      onChooseSaveRoot={() => {}}
      onImportAudioFiles={() => {}}
      onEnqueueBacklog={() => {}}
      onCancelCurrent={() => {}}
      onProcessNextTranscription={() => {}}
      onRetry={() => {}}
      onSkip={() => {}}
      onVerifyModel={() => {}}
      onDownloadModel={() => {}}
      onCancelModelDownload={() => {}}
      onDeleteModel={() => {}}
    />,
  );
}

function buttonTag(markup: string, label: string) {
  const match = markup.match(new RegExp(`<button[^>]*>[\\s\\S]*?${label}[\\s\\S]*?</button>`));
  expect(match, `expected ${label} button`).not.toBeNull();
  return match?.[0] ?? "";
}

function isDisabled(button: string) {
  return /\sdisabled(=""|\s|>)/.test(button);
}

describe("settings panel", () => {
  it("allows start input when a pinned microphone is missing but default fallback is available", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.selected_microphone = "input-9-missing-airpods";
    snapshot.settings.selected_microphone_label = "Missing AirPods";
    snapshot.microphones = [
      {
        id: "default",
        label: "System Default",
        available: true,
        fallback: true,
      },
      {
        id: "input-0-built-in-microphone",
        label: "Built-in Microphone",
        available: true,
        fallback: false,
      },
    ];

    const markup = renderSettingsPanel(snapshot);

    expect(isDisabled(buttonTag(markup, "Start Input"))).toBe(false);
    expect(markup).toContain("Missing AirPods is unavailable");
    expect(markup).toContain("Start Input will use System Default");
  });
});
