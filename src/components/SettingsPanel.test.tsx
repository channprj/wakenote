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
      onChooseModelDirectory={() => {}}
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
  const match = markup.match(
    new RegExp(`<button[^>]*>(?:(?!</button>)[\\s\\S])*?${label}(?:(?!</button>)[\\s\\S])*?</button>`),
  );
  expect(match, `expected ${label} button`).not.toBeNull();
  return match?.[0] ?? "";
}

function isDisabled(button: string) {
  return /\sdisabled(=""|\s|>)/.test(button);
}

function switchTag(markup: string, label: string) {
  const match = markup.match(new RegExp(`<button[^>]*aria-label="${label}"[^>]*>`));
  expect(match, `expected ${label} switch`).not.toBeNull();
  return match?.[0] ?? "";
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

  it("shows a transcription language selector in the general controls", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.transcription_language = "ko";

    const markup = renderSettingsPanel(snapshot);

    expect(markup).toContain("Transcription Language");
    expect(markup).toContain('<option value="auto">Auto-detect</option>');
    expect(markup).toContain('<option value="ko" selected="">Korean</option>');
  });

  it("shows an auto-start live input toggle in general controls", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.start_live_input_on_launch = false;

    const markup = renderSettingsPanel(snapshot);
    const autoStartSwitch = switchTag(markup, "Start input on launch");

    expect(autoStartSwitch).toContain('aria-checked="false"');
  });

  it("shows a low-confidence transcript suppression toggle in general controls", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.suppress_low_confidence_transcripts = false;

    const markup = renderSettingsPanel(snapshot);
    const suppressionSwitch = switchTag(markup, "Hide low-confidence transcripts");

    expect(suppressionSwitch).toContain('aria-checked="false"');
  });

  it("shows VAD gate as unavailable and forced off until VAD is implemented", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.vad_enabled = true;

    const markup = renderSettingsPanel(snapshot, "privacy");
    const vadSwitch = switchTag(markup, "VAD gate");

    expect(vadSwitch).toContain('aria-checked="false"');
    expect(isDisabled(vadSwitch)).toBe(true);
  });

  it("shows dock and menu bar icon visibility controls in advanced settings", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.show_dock_icon = false;
    snapshot.settings.show_tray_icon = false;

    const markup = renderSettingsPanel(snapshot, "advanced");
    const dockSwitch = switchTag(markup, "Show Dock icon");
    const menuBarSwitch = switchTag(markup, "Show menu bar icon");

    expect(dockSwitch).toContain('aria-checked="false"');
    expect(menuBarSwitch).toContain('aria-checked="false"');
  });

  it("shows quick max chunk duration presets for transcription-friendly chunking", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.max_chunk_ms = 120_000;

    const markup = renderSettingsPanel(snapshot, "recording");

    expect(markup).toContain("Max Chunk");
    expect(markup).toContain("2 min");
    expect(markup).toContain("1 min");
    expect(markup).toContain("3 min");
    expect(markup).toContain("5 min");
    expect(markup).toContain('aria-pressed="true"');
  });

  it("puts live level details above general capture controls and removes tray preview copy", () => {
    const snapshot = mockSnapshot();
    snapshot.status.tray_state = "error";

    const markup = renderSettingsPanel(snapshot);

    expect(markup).not.toContain("tray-preview");
    expect(markup.indexOf("Current")).toBeLessThan(markup.indexOf("Recording"));
    expect(markup.indexOf("Peak")).toBeLessThan(markup.indexOf("Recording"));
  });

  it("shows all transcripts grouped by day with recording file links", () => {
    const snapshot = mockSnapshot();
    snapshot.recent_transcripts = [
      {
        transcript_path: "/tmp/WakeNote/20260510/010203.txt",
        audio_path: "/tmp/WakeNote/20260510/010203.m4a",
        recorded_at: "2026-05-10T01:02:03+09:00",
        text: "daily transcript text",
      },
    ];

    const markup = renderSettingsPanel(snapshot, "transcripts");

    expect(markup).toContain("Transcripts");
    expect(markup).toContain("2026-05-10");
    expect(markup).toContain("daily transcript text");
    expect(markup).toContain('href="file:///tmp/WakeNote/20260510/010203.m4a"');
  });

  it("groups history jobs by recording day", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
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

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toContain("2026-05-10");
    expect(markup).toContain("/tmp/WakeNote/20260510/010203.m4a");
  });

  it("renders shortened audio path labels in the history queue with the full path in the tooltip", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/Users/me/Documents/WakeNote/20260510/010203.m4a",
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

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toContain('title="/Users/me/Documents/WakeNote/20260510/010203.m4a"');
    expect(markup).toMatch(/<td[^>]*>20260510\/010203\.m4a<\/td>/);
    expect(markup).not.toMatch(/<td[^>]*>\/Users\/me\/Documents\/WakeNote\/20260510\/010203\.m4a</);
  });

  it("renders the friendly model display name in the history queue and the models eyebrow", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
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

    const historyMarkup = renderSettingsPanel(snapshot, "history");

    expect(historyMarkup).toContain("Whisper Tiny");
    expect(historyMarkup).toContain('title="whisper-tiny"');
    expect(historyMarkup).not.toMatch(/<td[^>]*>whisper-tiny</);

    const modelsMarkup = renderSettingsPanel(snapshot, "models");

    expect(modelsMarkup).toContain("Whisper Medium");
    expect(modelsMarkup).not.toMatch(/ui-badge--primary[^>]*>whisper-medium</);
  });

  it("surfaces the queue job error as a hover tooltip on the status cell", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "model whisper-tiny is missing on disk",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/010204.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 1,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<td title="model whisper-tiny is missing on disk"[^>]*>\s*<span class="ui-badge ui-badge--danger[^"]*">failed<\/span>\s*<\/td>/,
    );
    expect(markup).toMatch(
      /<td>\s*<span class="ui-badge ui-badge--success[^"]*">completed<\/span>\s*<\/td>/,
    );
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
    snapshot.settings.save_root = "~/Documents/WakeNote";
    snapshot.settings.save_root_confirmed = false;

    const unconfirmedMarkup = renderSettingsPanel(snapshot, "storage");

    expect(isDisabled(buttonTag(unconfirmedMarkup, "Confirm Save Root"))).toBe(false);

    snapshot.settings.save_root = "   ";
    const blankMarkup = renderSettingsPanel(snapshot, "storage");

    expect(isDisabled(buttonTag(blankMarkup, "Confirm Save Root"))).toBe(true);

    snapshot.settings.save_root = "/tmp/wakenote-recordings";
    snapshot.settings.save_root_confirmed = true;
    const confirmedMarkup = renderSettingsPanel(snapshot, "storage");

    expect(confirmedMarkup).toContain("Confirmed");
    expect(confirmedMarkup).not.toContain("Confirm Save Root");
  });
});
