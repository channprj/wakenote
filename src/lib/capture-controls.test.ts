import { describe, expect, it } from "vitest";
import type { AppSettings, AppStatus, MicrophoneDevice } from "./types";
import {
  derivePriorityList,
  inputAvailability,
  normalizeCaptureMicrophones,
  rebindCaptureMicrophones,
  startLiveCaptureDisabledReason,
  stopLiveCaptureDisabledReason,
} from "./capture-controls";

const startSettings: Pick<AppSettings, "pause_all" | "recording_enabled"> = {
  pause_all: false,
  recording_enabled: true,
};
const stoppedStatus: Pick<AppStatus, "live_input_active" | "runtime_warning"> =
  {
    live_input_active: false,
    runtime_warning: null,
  };

describe("capture controls", () => {
  it("normalizes capture microphones to at most two slots", () => {
    const cases: Array<[string, Parameters<typeof normalizeCaptureMicrophones>[0], ReturnType<typeof normalizeCaptureMicrophones>]> = [
      [
        "keeps the same microphone on both slots and drops a third",
        [
          { id: "wired", label: "Wired" },
          { id: "wired", label: "Duplicate" },
          { id: "wireless", label: "Wireless" },
          { id: "third", label: "Third" },
        ],
        [
          { id: "wired", label: "Wired" },
          { id: "wired", label: "Duplicate" },
        ],
      ],
      [
        "allows System Default with a second device",
        [
          { id: "default", label: "System Default" },
          { id: "wired", label: "Wired" },
        ],
        [
          { id: "default", label: "System Default" },
          { id: "wired", label: "Wired" },
        ],
      ],
    ];

    for (const [label, input, expected] of cases) {
      expect(normalizeCaptureMicrophones(input), label).toEqual(expected);
    }
  });

  it("rebinds a stale BOYA selection to one active same-name device", () => {
    expect(
      rebindCaptureMicrophones(
        [
          {
            id: "input-4-boya-cm40",
            label: "BOYA CM40",
            core_audio_uid: "uid-stale",
          },
        ],
        [
          {
            id: "input-2-boya-cm40",
            label: "  boya   cm40 ",
            core_audio_uid: "uid-active",
            available: true,
            fallback: false,
          },
        ],
      ),
    ).toEqual([
      {
        id: "input-2-boya-cm40",
        label: "boya   cm40",
        core_audio_uid: "uid-active",
      },
    ]);
  });

  it("does not guess between two active devices with the same name", () => {
    expect(
      rebindCaptureMicrophones(
        [{ id: "input-4-boya-cm40", label: "BOYA CM40" }],
        [
          {
            id: "input-1-boya-cm40",
            label: "BOYA CM40",
            available: true,
            fallback: false,
          },
          {
            id: "input-2-boya-cm40",
            label: "BOYA CM40",
            available: true,
            fallback: false,
          },
        ],
      ),
    ).toEqual([{ id: "input-4-boya-cm40", label: "BOYA CM40" }]);
  });

  it("allows Start Input when capture and an input are available", () => {
    expect(
      startLiveCaptureDisabledReason(startSettings, stoppedStatus, true),
    ).toBeNull();
  });

  it("explains capture blockers in action priority order", () => {
    expect(
      startLiveCaptureDisabledReason(
        { ...startSettings, pause_all: true },
        stoppedStatus,
        true,
      ),
    ).toBe("All capture is paused");
    expect(
      startLiveCaptureDisabledReason(startSettings, stoppedStatus, false),
    ).toBe("No microphone available");
    expect(stopLiveCaptureDisabledReason({ live_input_active: false })).toBe(
      "Input is not running",
    );
  });
});

describe("inputAvailability", () => {
  const settings: Pick<
    AppSettings,
    "microphone_priority" | "selected_microphone" | "selected_microphone_label"
  > = {
    selected_microphone: "studio",
    selected_microphone_label: "Studio Mic",
    microphone_priority: [
      { id: "studio", label: "Studio Mic" },
      { id: "laptop", label: "MacBook Microphone" },
    ],
  };

  it("waits for the selected primary when it is unavailable or absent", () => {
    const cases: Array<[string, MicrophoneDevice[]]> = [
      // Another device is available but is not substituted for the primary.
      [
        "primary unavailable",
        [
          { id: "studio", label: "Studio Mic", available: false, fallback: false },
          {
            id: "laptop",
            label: "MacBook Microphone",
            available: true,
            fallback: false,
          },
        ],
      ],
      // No selected or fallback input is available at all.
      ["no microphones", []],
    ];

    for (const [label, microphones] of cases) {
      expect(inputAvailability(settings, microphones), label).toEqual({
        canStart: false,
        activeLabel: "Studio Mic",
        warning:
          'Primary input "Studio Mic" is unavailable. WakeNote will wait for the same device.',
        warningTone: "danger",
      });
    }
  });

  it("allows capture through the single active same-name replacement", () => {
    expect(
      inputAvailability(
        {
          ...settings,
          capture_microphones: [
            {
              id: "input-4-boya-cm40",
              label: "BOYA CM40",
              core_audio_uid: "uid-stale",
            },
          ],
        },
        [
          {
            id: "input-2-boya-cm40",
            label: "BOYA CM40",
            core_audio_uid: "uid-active",
            available: true,
            fallback: false,
          },
        ],
      ),
    ).toEqual({
      canStart: true,
      activeLabel: "BOYA CM40",
      warning: null,
      warningTone: null,
    });
  });

  it("resynchronizes the selected microphone at the top of priority", () => {
    expect(
      derivePriorityList({
        ...settings,
        microphone_priority: [
          { id: "laptop", label: "MacBook Microphone" },
          { id: "studio", label: "Studio Mic" },
        ],
      }),
    ).toEqual([
      { id: "studio", label: "Studio Mic" },
      { id: "laptop", label: "MacBook Microphone" },
    ]);
  });
});
