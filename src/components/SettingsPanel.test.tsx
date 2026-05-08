import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "../lib/app-state";
import { SettingsPanel } from "./SettingsPanel";
import type { AppSnapshot } from "../lib/types";

function renderSettingsPanel(snapshot: AppSnapshot, activeSection = "general") {
  return renderToStaticMarkup(
    <SettingsPanel
      activeSection={activeSection}
      snapshot={snapshot}
      onPatch={() => {}}
      onRefresh={() => {}}
      onStartLiveCapture={() => {}}
      onStopLiveCapture={() => {}}
      onChooseSaveRoot={() => {}}
      onRevealSaveFolder={() => {}}
      onImportAudioFiles={() => {}}
      onEnqueueBacklog={() => {}}
      onCancelCurrent={() => {}}
      onCancelCurrentOperation={() => {}}
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
  const match = markup.match(
    new RegExp(`<button[^>]*>(?:(?!</button>)[\\s\\S])*?${label}(?:(?!</button>)[\\s\\S])*?</button>`),
  );
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

  it("disables redundant live input start and stop actions while preserving error recovery", () => {
    const stopped = mockSnapshot();
    const stoppedMarkup = renderSettingsPanel(stopped);

    expect(isDisabled(buttonTag(stoppedMarkup, "Start Input"))).toBe(false);
    expect(isDisabled(buttonTag(stoppedMarkup, "Stop Input"))).toBe(true);

    const active = mockSnapshot();
    active.status.live_input_active = true;
    active.status.tray_state = "listening";
    const activeMarkup = renderSettingsPanel(active);

    expect(isDisabled(buttonTag(activeMarkup, "Start Input"))).toBe(true);
    expect(isDisabled(buttonTag(activeMarkup, "Stop Input"))).toBe(false);

    const errored = mockSnapshot();
    errored.status.live_input_active = true;
    errored.status.tray_state = "error";
    errored.status.runtime_warning = "Live input stream error: default input stream disconnected";
    const erroredMarkup = renderSettingsPanel(errored);

    expect(isDisabled(buttonTag(erroredMarkup, "Start Input"))).toBe(false);
    expect(isDisabled(buttonTag(erroredMarkup, "Stop Input"))).toBe(false);
  });

  it("disables process next until the selected model is usable", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/imported/pending.wav",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const missingMarkup = renderSettingsPanel(snapshot, "history");

    expect(isDisabled(buttonTag(missingMarkup, "Process Next"))).toBe(true);

    snapshot.models = snapshot.models.map((model) =>
      model.id === snapshot.settings.selected_model ? { ...model, status: "ready" } : model,
    );
    const readyMarkup = renderSettingsPanel(snapshot, "history");

    expect(isDisabled(buttonTag(readyMarkup, "Process Next"))).toBe(false);
  });

  it("disables process next until at least one pending job model is usable", () => {
    const snapshot = mockSnapshot();
    snapshot.models = snapshot.models.map((model) =>
      model.id === snapshot.settings.selected_model ? { ...model, status: "ready" } : model,
    );
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/imported/pending-tiny.wav",
          model_id: "whisper-tiny",
          status: "pending",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const missingJobModelMarkup = renderSettingsPanel(snapshot, "history");

    expect(isDisabled(buttonTag(missingJobModelMarkup, "Process Next"))).toBe(true);

    snapshot.models = snapshot.models.map((model) =>
      model.id === "whisper-tiny" ? { ...model, status: "ready" } : model,
    );
    const readyJobModelMarkup = renderSettingsPanel(snapshot, "history");

    expect(isDisabled(buttonTag(readyJobModelMarkup, "Process Next"))).toBe(false);
  });

  it("shows an explicit save root confirmation action until storage is confirmed", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.save_root = "~/Documents/Sagwan";
    snapshot.settings.save_root_confirmed = false;

    const unconfirmedMarkup = renderSettingsPanel(snapshot, "storage");

    expect(isDisabled(buttonTag(unconfirmedMarkup, "Confirm Save Root"))).toBe(false);

    snapshot.settings.save_root = "   ";
    const blankMarkup = renderSettingsPanel(snapshot, "storage");

    expect(isDisabled(buttonTag(blankMarkup, "Confirm Save Root"))).toBe(true);

    snapshot.settings.save_root = "/tmp/sagwan-recordings";
    snapshot.settings.save_root_confirmed = true;
    const confirmedMarkup = renderSettingsPanel(snapshot, "storage");

    expect(confirmedMarkup).toContain("Confirmed");
    expect(confirmedMarkup).not.toContain("Confirm Save Root");
  });
});
