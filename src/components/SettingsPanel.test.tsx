import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "../lib/app-state";
import {
  SettingsPanel,
  addMicrophonePriority,
  confirmSaveRootDisabledReason,
  derivePriorityList,
  removeMicrophonePriority,
  reorderMicrophonePriority,
  startLiveCaptureDisabledReason,
  stopLiveCaptureDisabledReason,
  vadGateDisabledReason,
} from "./SettingsPanel";
import { customSourceText, parseCustomSourceInput } from "./SystemAudioSettings";
import type {
  AppSnapshot,
  AppSettings,
  AppStatus,
  MicrophoneDevice,
  MicrophonePriorityEntry,
} from "../lib/types";

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
      onChooseModelDirectory={() => {}}
      onRequestMicrophonePermission={() => {}}
      onRequestScreenRecordingPermission={() => {}}
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

function localDateSegments(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return {
    dashed: `${year}-${month}-${day}`,
    compact: `${year}${month}${day}`,
  };
}

describe("settings panel", () => {
  it("shows the system-audio toggle and reveals the screen-recording permission row only when enabled", () => {
    const base = mockSnapshot();
    const off = renderSettingsPanel(base);
    expect(off).toContain("Capture system audio");
    expect(off).not.toContain("Capture system audio (Google Meet, YouTube)");
    expect(off).not.toContain("Screen Recording Permission");

    const on = renderSettingsPanel({
      ...base,
      settings: { ...base.settings, system_audio_enabled: true },
    });
    expect(on).toContain("Screen Recording Permission");
    expect(on).toContain("Add Source");
    expect(on).toContain("Zoom");
    expect(on).toContain("Google Meet or browser tabs with Meet in the title");
  });

  it("renders custom system-audio sources from settings", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.system_audio_enabled = true;
    snapshot.settings.custom_sources = [
      {
        id: "zoom",
        label: "Zoom",
        title_patterns: ["Zoom Meeting"],
        auto_prompt: true,
      },
    ];

    const markup = renderSettingsPanel(snapshot);

    expect(markup).toContain("Zoom");
    expect(markup).toContain("Zoom Meeting");
    expect(markup).toContain("Google Meet, Meet, Zoom Meeting");
    expect(markup.match(/aria-label="Custom source"/g) ?? []).toHaveLength(1);
    expect(markup).not.toContain("Window title patterns");
  });

  it("parses comma-separated custom source input into title patterns", () => {
    expect(parseCustomSourceInput("Google Meet, Meet, Zoom Meeting")).toEqual({
      label: "Google Meet",
      title_patterns: ["Google Meet", "Meet", "Zoom Meeting"],
    });
    expect(parseCustomSourceInput(" Zoom Meeting , zoom.us , ")).toEqual({
      label: "Zoom Meeting",
      title_patterns: ["Zoom Meeting", "zoom.us"],
    });
  });

  it("renders comma-separated custom source patterns back into the input", () => {
    expect(
      customSourceText({
        id: "custom-source-1",
        label: "Google Meet",
        title_patterns: ["Google Meet", "Meet"],
        auto_prompt: true,
      }),
    ).toBe("Google Meet, Meet");
  });

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

    const recordingMarkup = renderSettingsPanel(snapshot, "recording");
    const generalMarkup = renderSettingsPanel(snapshot);

    expect(isDisabled(buttonTag(recordingMarkup, "Start Input"))).toBe(false);
    expect(recordingMarkup).toContain("Missing AirPods is unavailable");
    expect(recordingMarkup).toContain("Start Input will use System Default");
    expect(recordingMarkup).toMatch(
      /<div class="warning-banner warning-banner--warning">\s*Missing AirPods is unavailable/,
    );
    expect(recordingMarkup).not.toMatch(
      /warning-banner--danger[^"]*">[\s\S]*?Missing AirPods is unavailable/,
    );
    expect(generalMarkup).not.toContain("Missing AirPods is unavailable");
  });

  it("flags the microphone banner as danger when no fallback input is available", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.selected_microphone = "input-9-missing-airpods";
    snapshot.settings.selected_microphone_label = "Missing AirPods";
    snapshot.microphones = [];

    const markup = renderSettingsPanel(snapshot, "recording");

    expect(markup).toMatch(
      /<div class="warning-banner warning-banner--danger">\s*No available input device is selected\.\s*<\/div>/,
    );
    expect(markup).not.toMatch(
      /<div class="warning-banner warning-banner--warning">\s*No available input device is selected/,
    );
  });

  it("omits the microphone warning banner entirely when the selected mic is available", () => {
    const snapshot = mockSnapshot();
    const markup = renderSettingsPanel(snapshot, "recording");

    expect(markup).not.toContain("is unavailable");
    expect(markup).not.toContain("No available input device is selected");
    expect(markup).not.toMatch(/warning-banner--danger/);
  });

  it("disables redundant live input start and stop actions while preserving error recovery", () => {
    const stopped = mockSnapshot();
    const stoppedMarkup = renderSettingsPanel(stopped, "recording");

    expect(isDisabled(buttonTag(stoppedMarkup, "Start Input"))).toBe(false);
    expect(isDisabled(buttonTag(stoppedMarkup, "Stop Input"))).toBe(true);

    const active = mockSnapshot();
    active.status.live_input_active = true;
    active.status.tray_state = "listening";
    const activeMarkup = renderSettingsPanel(active, "recording");

    expect(isDisabled(buttonTag(activeMarkup, "Start Input"))).toBe(true);
    expect(isDisabled(buttonTag(activeMarkup, "Stop Input"))).toBe(false);

    const errored = mockSnapshot();
    errored.status.live_input_active = true;
    errored.status.tray_state = "error";
    errored.status.runtime_warning = "Live input stream error: default input stream disconnected";
    const erroredMarkup = renderSettingsPanel(errored, "recording");

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

  it("shows a light and dark appearance selector in advanced settings", () => {
    const snapshot = mockSnapshot();
    const markup = renderSettingsPanel(snapshot, "advanced");

    expect(markup).toContain("Theme");
    expect(markup).toContain('<option value="dark" selected="">Dark</option>');
    expect(markup).toContain('<option value="light">Light</option>');
  });

  it("shows microphone permission status in general controls", () => {
    const snapshot = mockSnapshot();
    snapshot.permissions.microphone = {
      status: "granted",
      label: "Allowed",
      detail: "WakeNote can use the microphone.",
      can_request: false,
      can_open_settings: true,
    };

    const markup = renderSettingsPanel(snapshot);

    expect(markup).toContain("Microphone Permission");
    expect(markup).toContain("Allowed");
    expect(markup).toContain("WakeNote can use the microphone.");
  });

  it("renders an allow microphone action when permission has not been requested", () => {
    const snapshot = mockSnapshot();
    snapshot.permissions.microphone = {
      status: "not_determined",
      label: "Not requested",
      detail: "WakeNote needs microphone access before recording can start.",
      can_request: true,
      can_open_settings: false,
    };

    const markup = renderSettingsPanel(snapshot);

    expect(markup).toContain("Microphone Permission");
    expect(isDisabled(buttonTag(markup, "Allow Microphone"))).toBe(false);
    expect(markup).not.toContain("Open System Settings");
  });

  it("renders a system settings action when microphone permission is denied", () => {
    const snapshot = mockSnapshot();
    snapshot.permissions.microphone = {
      status: "denied",
      label: "Denied",
      detail: "Enable Microphone for WakeNote in System Settings.",
      can_request: false,
      can_open_settings: true,
    };

    const markup = renderSettingsPanel(snapshot);

    expect(markup).toContain("Denied");
    expect(isDisabled(buttonTag(markup, "Open System Settings"))).toBe(false);
    expect(markup).not.toContain(">Allow Microphone<");
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

    const markup = renderSettingsPanel(snapshot);
    const vadSwitch = switchTag(markup, "VAD gate");

    expect(vadSwitch).toContain('aria-checked="false"');
    expect(isDisabled(vadSwitch)).toBe(true);
  });

  it("surfaces a why-disabled tooltip on the VAD gate switch", () => {
    const snapshot = mockSnapshot();
    const markup = renderSettingsPanel(snapshot);
    const vadSwitch = switchTag(markup, "VAD gate");

    expect(vadSwitch).toContain(`title="${vadGateDisabledReason()}"`);
  });

  it("omits the why-disabled tooltip on enabled toggle rows", () => {
    const snapshot = mockSnapshot();
    const markup = renderSettingsPanel(snapshot);
    const launchSwitch = switchTag(markup, "Launch at login");

    expect(launchSwitch).not.toContain("title=");
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

  it("shows lead-in padding as an adjustable recording timing control", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.lead_in_padding_ms = 300;

    const markup = renderSettingsPanel(snapshot, "recording");

    expect(markup).toContain("Lead-in");
    expect(markup).toContain("300 ms");
    expect(markup).toContain('max="2000"');
  });

  it("omits live recording status and capture actions from general settings", () => {
    const snapshot = mockSnapshot();
    snapshot.status.tray_state = "error";

    const markup = renderSettingsPanel(snapshot);

    expect(markup).not.toContain("tray-preview");
    expect(markup).not.toContain("Current");
    expect(markup).not.toContain("Peak");
    expect(markup).not.toContain("Start Input");
    expect(markup).not.toContain("Stop Input");
    expect(markup).not.toContain("Refresh");
    expect(markup).toContain("Launch at login");
    expect(markup).toContain("VAD gate");
  });

  it("renders the transcripts section with the day calendar", () => {
    const snapshot = mockSnapshot();
    const today = localDateSegments(new Date());

    const markup = renderSettingsPanel(snapshot, "transcripts");

    expect(markup).toContain("Transcripts");
    expect(markup).toContain(today.dashed);
    expect(markup).toContain("transcript-pagination--calendar");
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

  it("renders a job count alongside the recording day in the history queue group row", () => {
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
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(/table-group-row[\s\S]*?2026-05-10 \xb7 2 jobs/);
    expect(markup).toMatch(/table-group-row[\s\S]*?2026-05-09 \xb7 1 job(?!s)/);
  });

  it("sorts history queue day groups newest first regardless of insertion order", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/imported/standalone.wav",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
      ],
      pending_count: 2,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");
    const newerDayIndex = markup.indexOf("2026-05-10 \xb7");
    const olderDayIndex = markup.indexOf("2026-05-09 \xb7");
    const importedDayIndex = markup.indexOf("Imported \xb7");

    expect(newerDayIndex).toBeGreaterThanOrEqual(0);
    expect(olderDayIndex).toBeGreaterThan(newerDayIndex);
    expect(importedDayIndex).toBeGreaterThan(olderDayIndex);
  });

  it("orders queue jobs within a day chronologically regardless of insertion order", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/183000.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/091500.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/120000.m4a",
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
    const earliestIndex = markup.indexOf("2026-05-10/09:15:00.m4a");
    const middleIndex = markup.indexOf("2026-05-10/12:00:00.m4a");
    const latestIndex = markup.indexOf("2026-05-10/18:30:00.m4a");

    expect(earliestIndex).toBeGreaterThanOrEqual(0);
    expect(middleIndex).toBeGreaterThan(earliestIndex);
    expect(latestIndex).toBeGreaterThan(middleIndex);
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
    expect(markup).toMatch(/<td><a [^>]*>2026-05-10\/01:02:03\.m4a<\/a><\/td>/);
    expect(markup).not.toMatch(/<td[^>]*>\/Users\/me\/Documents\/WakeNote\/20260510\/010203\.m4a</);
    // The Audio cell now uses YYYY-MM-DD to match the day-group row format;
    // the raw 20260510/ prefix must never reach the visible cell text.
    expect(markup).not.toMatch(/<td[^>]*>20260510\/01:02:03\.m4a</);
  });

  it("wraps the history queue Audio cell in a file:// link so users can open the recording", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/Users/me/Documents/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toContain(
      'href="file:///Users/me/Documents/WakeNote/20260510/010203.m4a"',
    );
    expect(markup).toMatch(
      /<a href="file:\/\/\/Users\/me\/Documents\/WakeNote\/20260510\/010203\.m4a" title="\/Users\/me\/Documents\/WakeNote\/20260510\/010203\.m4a">\s*2026-05-10\/01:02:03\.m4a\s*<\/a>/,
    );
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
      /<td title="model whisper-tiny is missing on disk"[^>]*>\s*<a [^>]*>\s*<span class="ui-badge ui-badge--danger[^"]*">Failed<\/span>\s*<\/a>\s*<\/td>/,
    );
    expect(markup).toMatch(
      /<td>\s*<a [^>]*>\s*<span class="ui-badge ui-badge--success[^"]*">Completed<\/span>\s*<\/a>\s*<\/td>/,
    );
  });

  it("wraps the history queue status badge in a link to the transcript sidecar", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/Users/me/Documents/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 2,
          audio_path: "/Users/me/Documents/WakeNote/20260510/010204.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "decoder error",
        },
        {
          id: 3,
          audio_path: "/Users/me/Documents/WakeNote/20260510/010205.m4a",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 0,
      failed_count: 1,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    // Completed → links to .txt sidecar with raw path as hover tooltip.
    expect(markup).toMatch(
      /<a href="file:\/\/\/Users\/me\/Documents\/WakeNote\/20260510\/010203\.txt" title="\/Users\/me\/Documents\/WakeNote\/20260510\/010203\.txt">\s*<span class="ui-badge ui-badge--success[^"]*">Completed<\/span>\s*<\/a>/,
    );
    // Failed → links to .error.txt sidecar; the row's existing error tooltip stays on the td.
    expect(markup).toMatch(
      /<a href="file:\/\/\/Users\/me\/Documents\/WakeNote\/20260510\/010204\.error\.txt" title="\/Users\/me\/Documents\/WakeNote\/20260510\/010204\.error\.txt">\s*<span class="ui-badge ui-badge--danger[^"]*">Failed<\/span>\s*<\/a>/,
    );
    // Pending → no sidecar yet, so the badge renders unwrapped.
    expect(markup).toMatch(
      /<td>\s*<span class="ui-badge ui-badge--neutral[^"]*">Pending<\/span>\s*<\/td>/,
    );
  });

  it("title-cases every QueueJobStatus value in the per-row Badge text", () => {
    const snapshot = mockSnapshot();
    const statuses: { id: number; status: "pending" | "running" | "completed" | "failed" | "cancelled" | "skipped"; label: string }[] = [
      { id: 1, status: "pending", label: "Pending" },
      { id: 2, status: "running", label: "Running" },
      { id: 3, status: "completed", label: "Completed" },
      { id: 4, status: "failed", label: "Failed" },
      { id: 5, status: "cancelled", label: "Cancelled" },
      { id: 6, status: "skipped", label: "Skipped" },
    ];
    snapshot.queue = {
      jobs: statuses.map(({ id, status }) => ({
        id,
        audio_path: `/tmp/WakeNote/20260510/0102${String(id).padStart(2, "0")}.m4a`,
        model_id: snapshot.settings.selected_model,
        status,
        error: null,
      })),
      pending_count: 1,
      running_count: 1,
      failed_count: 1,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    for (const { status, label } of statuses) {
      expect(markup).toMatch(
        new RegExp(`<span class="ui-badge ui-badge--[^"]+">${label}</span>`),
      );
      expect(markup).not.toMatch(
        new RegExp(`<span class="ui-badge ui-badge--[^"]+">${status}</span>`),
      );
    }
  });

  it("appends a pending count to the day group row only when pending jobs exist that day", () => {
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
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "boom",
        },
        {
          id: 4,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 2,
      running_count: 0,
      failed_count: 1,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /table-group-row[\s\S]*?2026-05-10 \xb7 3 jobs[\s\S]*?2 pending[\s\S]*?1 failed/,
    );
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("appends a failed count to the day group row only when failures exist that day", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "boom",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 4,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 2,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(/table-group-row[\s\S]*?2026-05-10 \xb7 3 jobs[\s\S]*?2 failed/);
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("renders the day group row pending count in warning tone when pending jobs exist", () => {
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
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /table-group-row[\s\S]*?<span data-tone="warning">1 pending<\/span>/,
    );
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("renders the day group row failed count in danger tone when failures exist", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "boom",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
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
      /table-group-row[\s\S]*?<span data-tone="danger">1 failed<\/span>/,
    );
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("appends a cancelled count to the day group row only when cancelled jobs exist that day", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 4,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(/table-group-row[\s\S]*?2026-05-10 \xb7 3 jobs[\s\S]*?2 cancelled/);
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("renders the day group row cancelled count in danger tone when cancellations exist", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /table-group-row[\s\S]*?<span data-tone="danger">1 cancelled<\/span>/,
    );
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("appends a skipped count to the day group row only when skipped jobs exist that day", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model missing",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 4,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(/table-group-row[\s\S]*?2026-05-10 \xb7 3 jobs[\s\S]*?2 skipped/);
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("renders the day group row skipped count in warning tone when skips exist", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model missing",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /table-group-row[\s\S]*?<span data-tone="warning">1 skipped<\/span>/,
    );
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("renders the day group row completed count in success tone when completed jobs exist", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
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

    expect(markup).toMatch(
      /table-group-row[\s\S]*?<span data-tone="success">2 completed<\/span>/,
    );
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="warning">1 pending<\/span><\/td>/,
    );
  });

  it("renders the day group row running count in primary tone when running jobs exist", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "running",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "running",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/235959.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 2,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /table-group-row[\s\S]*?<span data-tone="primary">2 running<\/span>/,
    );
    expect(markup).toMatch(
      /<tr class="table-group-row"><td colSpan="4">2026-05-09 \xb7 1 job \xb7 <span data-tone="success">1 completed<\/span><\/td>/,
    );
  });

  it("surfaces a completed count in the queue stats banner alongside pending/running/failed", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 4,
          audio_path: "/tmp/WakeNote/20260510/040506.m4a",
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

    expect(markup).toMatch(
      /<div class="queue-stats">[\s\S]*?<span>Completed<\/span>\s*<strong>3<\/strong>/,
    );
    expect(markup).toMatch(/<span>Pending<\/span>\s*<strong>1<\/strong>/);
  });

  it("renders zero completed jobs in the queue stats banner when no jobs have finished", () => {
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

    expect(markup).toMatch(
      /<div class="queue-stats">[\s\S]*?<span>Completed<\/span>\s*<strong>0<\/strong>/,
    );
  });

  it("tones the queue-stats Failed and Completed cells when their counts are non-zero", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "boom",
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 1,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<div data-tone="danger"[^>]*><span>Failed<\/span>\s*<strong>1<\/strong>/,
    );
    expect(markup).toMatch(
      /<div data-tone="success"[^>]*><span>Completed<\/span>\s*<strong>1<\/strong>/,
    );
  });

  it("leaves the queue-stats Failed and Completed cells untoned on a clean queue", () => {
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

    expect(markup).not.toMatch(/data-tone="danger"><span>Failed/);
    expect(markup).not.toMatch(/data-tone="success"><span>Completed/);
    expect(markup).toMatch(/<div><span>Failed<\/span>\s*<strong>0<\/strong>/);
    expect(markup).toMatch(/<div><span>Completed<\/span>\s*<strong>0<\/strong>/);
  });

  it("tones the queue-stats Pending cell with warning when the count is non-zero", () => {
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

    expect(markup).toMatch(
      /<div data-tone="warning"[^>]*><span>Pending<\/span>\s*<strong>1<\/strong>/,
    );
  });

  it("leaves the queue-stats Pending cell untoned when no jobs are pending", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).not.toMatch(/data-tone="warning"><span>Pending/);
    expect(markup).toMatch(/<div><span>Pending<\/span>\s*<strong>0<\/strong>/);
  });

  it("tones the queue-stats Running cell with primary when the count is non-zero", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "running",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 1,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<div data-tone="primary"[^>]*><span>Running<\/span>\s*<strong>1<\/strong>/,
    );
  });

  it("leaves the queue-stats Running cell untoned when nothing is running", () => {
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

    expect(markup).not.toMatch(/data-tone="primary"><span>Running/);
    expect(markup).toMatch(/<div><span>Running<\/span>\s*<strong>0<\/strong>/);
  });

  it("exposes a per-day breakdown tooltip on each queue-stats cell with jobs", () => {
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
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/170000.m4a",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
        {
          id: 4,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 3,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /title="2026-05-10: 2 \xb7 2026-05-09: 1"><span>Pending<\/span>/,
    );
    expect(markup).toMatch(
      /title="2026-05-10: 1"><span>Completed<\/span>/,
    );
  });

  it("omits queue-stats cell tooltips when no jobs are in that status", () => {
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

    expect(markup).not.toMatch(/title="[^"]*"><span>Running/);
    expect(markup).not.toMatch(/title="[^"]*"><span>Failed/);
    expect(markup).not.toMatch(/title="[^"]*"><span>Cancelled/);
    expect(markup).not.toMatch(/title="[^"]*"><span>Skipped/);
    expect(markup).not.toMatch(/title="[^"]*"><span>Completed/);
  });

  it("renders a Cancelled cell in the queue stats banner counting cancelled jobs", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<div class="queue-stats">[\s\S]*?<span>Cancelled<\/span>\s*<strong>2<\/strong>/,
    );
  });

  it("tones the queue-stats Cancelled cell with danger when the count is non-zero", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<div data-tone="danger"[^>]*><span>Cancelled<\/span>\s*<strong>1<\/strong>/,
    );
  });

  it("leaves the queue-stats Cancelled cell untoned when no jobs are cancelled", () => {
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

    expect(markup).not.toMatch(/data-tone="danger"><span>Cancelled/);
    expect(markup).toMatch(/<div><span>Cancelled<\/span>\s*<strong>0<\/strong>/);
  });

  it("exposes a per-day breakdown tooltip on the Cancelled cell when cancelled jobs exist", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/170000.m4a",
          model_id: snapshot.settings.selected_model,
          status: "cancelled",
          error: "cancelled by user",
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /title="2026-05-10: 2 \xb7 2026-05-09: 1"><span>Cancelled<\/span>/,
    );
  });

  it("renders a Skipped cell in the queue stats banner counting skipped jobs", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model unavailable",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model unavailable",
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<div class="queue-stats">[\s\S]*?<span>Skipped<\/span>\s*<strong>2<\/strong>/,
    );
  });

  it("tones the queue-stats Skipped cell with warning when the count is non-zero", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model unavailable",
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<div data-tone="warning"[^>]*><span>Skipped<\/span>\s*<strong>1<\/strong>/,
    );
  });

  it("leaves the queue-stats Skipped cell untoned when no jobs are skipped", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).not.toMatch(/data-tone="warning"><span>Skipped/);
    expect(markup).toMatch(/<div><span>Skipped<\/span>\s*<strong>0<\/strong>/);
  });

  it("exposes a per-day breakdown tooltip on the Skipped cell when skipped jobs exist", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model unavailable",
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model unavailable",
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260509/170000.m4a",
          model_id: snapshot.settings.selected_model,
          status: "skipped",
          error: "model unavailable",
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /title="2026-05-10: 2 \xb7 2026-05-09: 1"><span>Skipped<\/span>/,
    );
  });

  it("decorates each queue row with a status-derived data-tone for left-edge accent styling", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/WakeNote/20260510/010203.m4a",
          model_id: snapshot.settings.selected_model,
          status: "running",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "boom",
        },
        {
          id: 3,
          audio_path: "/tmp/WakeNote/20260510/030405.m4a",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 4,
          audio_path: "/tmp/WakeNote/20260510/040506.m4a",
          model_id: snapshot.settings.selected_model,
          status: "pending",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 1,
      failed_count: 1,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toMatch(
      /<tr data-tone="primary"><td><a [^>]*>2026-05-10\/01:02:03\.m4a<\/a><\/td>/,
    );
    expect(markup).toMatch(
      /<tr data-tone="danger"><td><a [^>]*>2026-05-10\/02:03:04\.m4a<\/a><\/td>/,
    );
    expect(markup).toMatch(
      /<tr data-tone="success"><td><a [^>]*>2026-05-10\/03:04:05\.m4a<\/a><\/td>/,
    );
    // Pending rows stay untoned (neutral) so they don't compete with actionable
    // rows for visual attention; assert the tr opens with just the React key
    // markup and no data-tone attribute precedes the Audio cell.
    expect(markup).toMatch(
      /<tr><td><a [^>]*>2026-05-10\/04:05:06\.m4a<\/a><\/td>/,
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

  it("annotates disabled queue toolbar buttons with the reason on hover", () => {
    const snapshot = mockSnapshot();
    snapshot.models = snapshot.models.map((model) =>
      model.id === snapshot.settings.selected_model ? { ...model, status: "ready" } : model,
    );

    snapshot.queue = {
      jobs: [],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;
    const emptyMarkup = renderSettingsPanel(snapshot, "history");
    expect(buttonTag(emptyMarkup, "Process Next")).toContain('title="No pending jobs"');
    expect(buttonTag(emptyMarkup, "Cancel Current")).toContain(
      'title="No running job to cancel"',
    );

    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/imported/in-flight.wav",
          model_id: snapshot.settings.selected_model,
          status: "running",
          error: null,
        },
      ],
      pending_count: 1,
      running_count: 1,
      failed_count: 0,
    };
    snapshot.status.queue = snapshot.queue;
    const runningMarkup = renderSettingsPanel(snapshot, "history");
    expect(buttonTag(runningMarkup, "Process Next")).toContain(
      'title="A job is already running"',
    );
    expect(buttonTag(runningMarkup, "Cancel Current")).not.toMatch(/\stitle="/);

    snapshot.settings.pause_all = true;
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
    const pausedMarkup = renderSettingsPanel(snapshot, "history");
    expect(buttonTag(pausedMarkup, "Process Next")).toContain(
      'title="Transcription unavailable"',
    );

    snapshot.settings.pause_all = false;
    const readyMarkup = renderSettingsPanel(snapshot, "history");
    expect(isDisabled(buttonTag(readyMarkup, "Process Next"))).toBe(false);
    expect(buttonTag(readyMarkup, "Process Next")).not.toMatch(/\stitle="/);
  });

  it("annotates disabled per-row Retry and Skip buttons with the reason on hover", () => {
    const snapshot = mockSnapshot();
    snapshot.queue = {
      jobs: [
        {
          id: 1,
          audio_path: "/tmp/imported/running.wav",
          model_id: snapshot.settings.selected_model,
          status: "running",
          error: null,
        },
        {
          id: 2,
          audio_path: "/tmp/imported/done.wav",
          model_id: snapshot.settings.selected_model,
          status: "completed",
          error: null,
        },
        {
          id: 3,
          audio_path: "/tmp/imported/failed.wav",
          model_id: snapshot.settings.selected_model,
          status: "failed",
          error: "boom",
        },
      ],
      pending_count: 0,
      running_count: 1,
      failed_count: 1,
    };
    snapshot.status.queue = snapshot.queue;

    const markup = renderSettingsPanel(snapshot, "history");

    expect(markup).toContain('title="Job is still running"');
    expect(markup).toContain('title="Job already completed"');
    expect(markup).toMatch(/title="Retry"[^>]*>(?:(?!<\/button>)[\s\S])*?<svg/);
    expect(markup).toMatch(/title="Skip"[^>]*>(?:(?!<\/button>)[\s\S])*?<svg/);
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

  it("wraps the save root in a file:// link inside each path-pattern code block", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.save_root = "/tmp/wakenote-recordings";
    snapshot.settings.audio_format = "m4a";

    const markup = renderSettingsPanel(snapshot, "storage");

    expect(markup).toContain(
      '<code><a href="file:///tmp/wakenote-recordings" title="/tmp/wakenote-recordings">/tmp/wakenote-recordings</a>/YYYYMMDD/HHMMSS.m4a</code>',
    );
    expect(markup).toContain(
      '<code><a href="file:///tmp/wakenote-recordings" title="/tmp/wakenote-recordings">/tmp/wakenote-recordings</a>/YYYYMMDD/HHMMSS.txt</code>',
    );
    expect(markup).toContain(
      '<code><a href="file:///tmp/wakenote-recordings" title="/tmp/wakenote-recordings">/tmp/wakenote-recordings</a>/YYYYMMDD/HHMMSS.json</code>',
    );
  });

  it("offers m4a, mp3, and wav as recording formats", () => {
    const markup = renderSettingsPanel(mockSnapshot(), "storage");

    expect(markup).toContain('<option value="m4a" selected="">M4A / AAC</option>');
    expect(markup).toContain('<option value="mp3">MP3</option>');
    expect(markup).toContain('<option value="wav">WAV / PCM</option>');
  });

  it("offers compressed audio bitrate presets with 96 kbps selected by default", () => {
    const markup = renderSettingsPanel(mockSnapshot(), "storage");

    expect(markup).toContain("Compressed Bitrate");
    expect(markup).toContain('<option value="64">64 kbps</option>');
    expect(markup).toContain('<option value="96" selected="">96 kbps</option>');
    expect(markup).toContain('<option value="128">128 kbps</option>');
  });

  it("disables compressed bitrate controls for wav recordings", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.audio_format = "wav";

    const markup = renderSettingsPanel(snapshot, "storage");

    expect(markup).toMatch(
      /<select class="ui-select" disabled="" title="WAV uses uncompressed PCM">/,
    );
  });

  it("renders Open Save Folder only in the storage section", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.save_root = "/tmp/wakenote-recordings";

    const generalMarkup = renderSettingsPanel(snapshot);
    const storageMarkup = renderSettingsPanel(snapshot, "storage");

    expect(generalMarkup).not.toContain("Open Save Folder");
    expect(storageMarkup).toContain("Open Save Folder");
    expect(buttonTag(storageMarkup, "Open Save Folder")).toContain(
      'title="/tmp/wakenote-recordings"',
    );
  });

  it("omits the file:// link wrap when save root is blank", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.save_root = "   ";

    const markup = renderSettingsPanel(snapshot, "storage");

    expect(markup).not.toContain('<a href="file://');
    expect(markup).toContain("<code>   /YYYYMMDD/HHMMSS.");
  });

  it("derives why-disabled tooltip text for the Start Input button across blocker states", () => {
    type StartSettings = Pick<AppSettings, "pause_all" | "recording_enabled">;
    type StartStatus = Pick<AppStatus, "live_input_active" | "runtime_warning">;
    const baseSettings: StartSettings = { pause_all: false, recording_enabled: true };
    const baseStatus: StartStatus = { live_input_active: false, runtime_warning: null };

    expect(startLiveCaptureDisabledReason(baseSettings, baseStatus, true)).toBeNull();
    expect(
      startLiveCaptureDisabledReason(
        baseSettings,
        { live_input_active: true, runtime_warning: null },
        true,
      ),
    ).toBe("Input is already running");
    expect(
      startLiveCaptureDisabledReason(
        baseSettings,
        {
          live_input_active: true,
          runtime_warning: "Live input stream error: default input stream disconnected",
        },
        true,
      ),
    ).toBeNull();
    expect(
      startLiveCaptureDisabledReason({ ...baseSettings, pause_all: true }, baseStatus, true),
    ).toBe("All capture is paused");
    expect(
      startLiveCaptureDisabledReason(
        { ...baseSettings, recording_enabled: false },
        baseStatus,
        true,
      ),
    ).toBe("Recording is disabled");
    expect(startLiveCaptureDisabledReason(baseSettings, baseStatus, false)).toBe(
      "No microphone available",
    );

    // priority: live_input_active comes before pause_all
    expect(
      startLiveCaptureDisabledReason(
        { pause_all: true, recording_enabled: true },
        { live_input_active: true, runtime_warning: null },
        true,
      ),
    ).toBe("Input is already running");
  });

  it("derives why-disabled tooltip text for the Stop Input button", () => {
    expect(stopLiveCaptureDisabledReason({ live_input_active: false })).toBe("Input is not running");
    expect(stopLiveCaptureDisabledReason({ live_input_active: true })).toBeNull();
  });

  it("derives why-disabled tooltip text for the Confirm Save Root button", () => {
    expect(confirmSaveRootDisabledReason({ save_root: "" })).toBe("Enter a save folder first");
    expect(confirmSaveRootDisabledReason({ save_root: "   " })).toBe("Enter a save folder first");
    expect(confirmSaveRootDisabledReason({ save_root: "~/Documents/WakeNote" })).toBeNull();
  });

  it("derives why-disabled tooltip text for the VAD gate toggle", () => {
    expect(vadGateDisabledReason()).toBe(
      "Planned for v1 — Silero VAD will gate non-speech noise",
    );
  });

  it("renders why-disabled tooltips on Start Input, Stop Input, and Confirm Save Root buttons", () => {
    const stopped = mockSnapshot();
    const stoppedMarkup = renderSettingsPanel(stopped, "recording");
    const stopButton = buttonTag(stoppedMarkup, "Stop Input");
    expect(stopButton).toContain('title="Input is not running"');
    // Start Input has no reason here (idle, mic available, recording on, not paused)
    expect(buttonTag(stoppedMarkup, "Start Input")).not.toContain("title=");

    const paused = mockSnapshot();
    paused.settings.pause_all = true;
    const pausedMarkup = renderSettingsPanel(paused, "recording");
    expect(buttonTag(pausedMarkup, "Start Input")).toContain('title="All capture is paused"');

    const active = mockSnapshot();
    active.status.live_input_active = true;
    active.status.tray_state = "listening";
    const activeMarkup = renderSettingsPanel(active, "recording");
    expect(buttonTag(activeMarkup, "Start Input")).toContain('title="Input is already running"');
    expect(buttonTag(activeMarkup, "Stop Input")).not.toContain("title=");

    const blankSaveRoot = mockSnapshot();
    blankSaveRoot.settings.save_root = "   ";
    blankSaveRoot.settings.save_root_confirmed = false;
    const blankMarkup = renderSettingsPanel(blankSaveRoot, "storage");
    expect(buttonTag(blankMarkup, "Confirm Save Root")).toContain(
      'title="Enter a save folder first"',
    );

    const validSaveRoot = mockSnapshot();
    validSaveRoot.settings.save_root = "~/Documents/WakeNote";
    validSaveRoot.settings.save_root_confirmed = false;
    const validMarkup = renderSettingsPanel(validSaveRoot, "storage");
    expect(buttonTag(validMarkup, "Confirm Save Root")).not.toContain("title=");
  });

  it.each([
    ["ready", "success"],
    ["installed", "success"],
    ["downloading", "primary"],
    ["verifying", "primary"],
    ["extracting", "primary"],
    ["error", "danger"],
    ["missing", "neutral"],
    ["unloaded", "neutral"],
  ] as const)(
    "wires the Models section header Badge tone to the selected model's %s status (→ %s)",
    (status, expectedTone) => {
      const snapshot = mockSnapshot();
      snapshot.models = snapshot.models.map((model) =>
        model.id === snapshot.settings.selected_model ? { ...model, status } : model,
      );
      const markup = renderSettingsPanel(snapshot, "models");
      expect(markup).toMatch(
        new RegExp(`<span class="ui-badge ui-badge--${expectedTone}">Whisper Medium</span>`),
      );
    },
  );

  it("falls back to neutral on the Models section header Badge when the selected model is not in the registry", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.selected_model = "whisper-removed-from-registry";
    const markup = renderSettingsPanel(snapshot, "models");
    expect(markup).toMatch(
      /<span class="ui-badge ui-badge--neutral">whisper-removed-from-registry<\/span>/,
    );
  });
});

describe("microphone priority list", () => {
  const samplePriority: MicrophonePriorityEntry[] = [
    { id: "input-3-usb-mic", label: "USB Mic" },
    { id: "input-1-built-in", label: "Built-in" },
    { id: "default", label: "System Default" },
  ];

  it("renders the priority list with the top entry tagged Primary and a hint about the 10-minute retry", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.microphone_priority = samplePriority;
    snapshot.settings.selected_microphone = "input-3-usb-mic";
    snapshot.settings.selected_microphone_label = "USB Mic";
    snapshot.microphones = [
      { id: "default", label: "System Default", available: true, fallback: false },
      { id: "input-3-usb-mic", label: "USB Mic", available: true, fallback: false },
      { id: "input-1-built-in", label: "Built-in", available: true, fallback: false },
    ];

    const markup = renderSettingsPanel(snapshot, "recording");

    expect(markup).toContain('aria-label="Microphone priority order"');
    expect(markup).toContain('<span class="ui-badge ui-badge--primary">Primary</span>');
    expect(markup).toContain("retries position 1 every 10 minutes");
    // The list renders the three entries in priority order.
    expect(markup.indexOf("USB Mic")).toBeLessThan(markup.indexOf("Built-in"));
    expect(markup.indexOf("Built-in")).toBeLessThan(markup.indexOf("System Default"));
  });

  it("disables the up arrow on the top entry and the down arrow on the bottom entry", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.microphone_priority = samplePriority;
    snapshot.settings.selected_microphone = "input-3-usb-mic";
    snapshot.settings.selected_microphone_label = "USB Mic";
    snapshot.microphones = [
      { id: "default", label: "System Default", available: true, fallback: false },
      { id: "input-3-usb-mic", label: "USB Mic", available: true, fallback: false },
      { id: "input-1-built-in", label: "Built-in", available: true, fallback: false },
    ];

    const markup = renderSettingsPanel(snapshot, "recording");
    const moveUsbUp = markup.match(
      /<button[^>]*aria-label="Move USB Mic up"[^>]*>/,
    )?.[0] ?? "";
    const moveDefaultDown = markup.match(
      /<button[^>]*aria-label="Move System Default down"[^>]*>/,
    )?.[0] ?? "";

    expect(isDisabled(moveUsbUp)).toBe(true);
    expect(isDisabled(moveDefaultDown)).toBe(true);
  });

  it("flags entries no longer in the connected device list as Not connected", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.microphone_priority = [
      { id: "input-9-ghost", label: "Ghost AirPods" },
      { id: "default", label: "System Default" },
    ];
    snapshot.settings.selected_microphone = "input-9-ghost";
    snapshot.settings.selected_microphone_label = "Ghost AirPods";
    snapshot.microphones = [
      { id: "default", label: "System Default", available: true, fallback: true },
    ];

    const markup = renderSettingsPanel(snapshot, "recording");

    expect(markup).toContain('<span class="ui-badge ui-badge--warning">Not connected</span>');
  });
});

describe("microphone priority list helpers", () => {
  const list: MicrophonePriorityEntry[] = [
    { id: "a", label: "Alpha" },
    { id: "b", label: "Bravo" },
    { id: "c", label: "Charlie" },
  ];

  it("reorders an entry up by swapping it with its predecessor", () => {
    expect(reorderMicrophonePriority(list, 1, 0)).toEqual([
      { id: "b", label: "Bravo" },
      { id: "a", label: "Alpha" },
      { id: "c", label: "Charlie" },
    ]);
  });

  it("reorders an entry down by swapping it with its successor", () => {
    expect(reorderMicrophonePriority(list, 0, 1)).toEqual([
      { id: "b", label: "Bravo" },
      { id: "a", label: "Alpha" },
      { id: "c", label: "Charlie" },
    ]);
  });

  it("returns the original list when the reorder is a no-op or out of bounds", () => {
    expect(reorderMicrophonePriority(list, 0, 0)).toBe(list);
    expect(reorderMicrophonePriority(list, 0, -1)).toBe(list);
    expect(reorderMicrophonePriority(list, 0, list.length)).toBe(list);
  });

  it("appends a new device that is not already in the list", () => {
    const device: Pick<MicrophoneDevice, "id" | "label"> = { id: "d", label: "Delta" };
    expect(addMicrophonePriority(list, device)).toEqual([
      ...list,
      { id: "d", label: "Delta" },
    ]);
  });

  it("ignores adding a device that is already in the list", () => {
    expect(addMicrophonePriority(list, { id: "b", label: "Bravo" })).toBe(list);
  });

  it("removes a non-last entry by index", () => {
    expect(removeMicrophonePriority(list, 1)).toEqual([
      { id: "a", label: "Alpha" },
      { id: "c", label: "Charlie" },
    ]);
  });

  it("refuses to remove the last remaining entry", () => {
    const single: MicrophonePriorityEntry[] = [{ id: "a", label: "Alpha" }];
    expect(removeMicrophonePriority(single, 0)).toBe(single);
  });

  it("derivePriorityList resyncs the priority list when the legacy mirror points elsewhere", () => {
    const settings: Pick<
      AppSettings,
      "microphone_priority" | "selected_microphone" | "selected_microphone_label"
    > = {
      microphone_priority: [
        { id: "default", label: "System Default" },
        { id: "input-3-usb-mic", label: "USB Mic" },
      ],
      selected_microphone: "input-3-usb-mic",
      selected_microphone_label: "USB Mic",
    };

    expect(derivePriorityList(settings)).toEqual([
      { id: "input-3-usb-mic", label: "USB Mic" },
      { id: "default", label: "System Default" },
    ]);
  });

  it("derivePriorityList synthesises a single-entry list when the priority field is empty", () => {
    expect(
      derivePriorityList({
        microphone_priority: [],
        selected_microphone: "default",
        selected_microphone_label: "System Default",
      }),
    ).toEqual([{ id: "default", label: "System Default" }]);
  });
});
