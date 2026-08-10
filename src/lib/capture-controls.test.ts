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
  it("keeps at most two unique explicit microphone selections", () => {
    expect(
      normalizeCaptureMicrophones([
        { id: "wired", label: "Wired" },
        { id: "wired", label: "Duplicate" },
        { id: "wireless", label: "Wireless" },
        { id: "third", label: "Third" },
      ]),
    ).toEqual([
      { id: "wired", label: "Wired" },
      { id: "wireless", label: "Wireless" },
    ]);
  });

  it("does not combine System Default with a second device", () => {
    expect(
      normalizeCaptureMicrophones([
        { id: "default", label: "System Default" },
        { id: "wired", label: "Wired" },
      ]),
    ).toEqual([{ id: "default", label: "System Default" }]);
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

  it("waits for the selected primary instead of substituting another device", () => {
    const microphones: MicrophoneDevice[] = [
      { id: "studio", label: "Studio Mic", available: false, fallback: false },
      {
        id: "laptop",
        label: "MacBook Microphone",
        available: true,
        fallback: false,
      },
    ];

    expect(inputAvailability(settings, microphones)).toEqual({
      canStart: false,
      activeLabel: "Studio Mic",
      warning:
        'Primary input "Studio Mic" is unavailable. WakeNote will wait for the same device.',
      warningTone: "danger",
    });
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

  it("surfaces a danger when no selected or fallback input is available", () => {
    expect(inputAvailability(settings, [])).toEqual({
      canStart: false,
      activeLabel: "Studio Mic",
      warning:
        'Primary input "Studio Mic" is unavailable. WakeNote will wait for the same device.',
      warningTone: "danger",
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
