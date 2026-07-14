import { describe, expect, it } from "vitest";
import type { AppSettings, AppStatus, MicrophoneDevice } from "./types";
import {
  derivePriorityList,
  inputAvailability,
  startLiveCaptureDisabledReason,
  stopLiveCaptureDisabledReason,
} from "./capture-controls";

const startSettings: Pick<AppSettings, "pause_all" | "recording_enabled"> = {
  pause_all: false,
  recording_enabled: true,
};
const stoppedStatus: Pick<AppStatus, "live_input_active" | "runtime_warning"> = {
  live_input_active: false,
  runtime_warning: null,
};

describe("capture controls", () => {
  it("allows Start Input when capture and an input are available", () => {
    expect(startLiveCaptureDisabledReason(startSettings, stoppedStatus, true)).toBeNull();
  });

  it("explains capture blockers in action priority order", () => {
    expect(
      startLiveCaptureDisabledReason(
        { ...startSettings, pause_all: true },
        stoppedStatus,
        true,
      ),
    ).toBe("All capture is paused");
    expect(startLiveCaptureDisabledReason(startSettings, stoppedStatus, false)).toBe(
      "No microphone available",
    );
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

  it("uses the first available priority entry and warns about fallback", () => {
    const microphones: MicrophoneDevice[] = [
      { id: "studio", label: "Studio Mic", available: false, fallback: false },
      { id: "laptop", label: "MacBook Microphone", available: true, fallback: false },
    ];

    expect(inputAvailability(settings, microphones)).toEqual({
      canStart: true,
      activeLabel: "MacBook Microphone",
      warning: 'Primary input "Studio Mic" is unavailable. WakeNote will use "MacBook Microphone".',
      warningTone: "warning",
    });
  });

  it("surfaces a danger when no selected or fallback input is available", () => {
    expect(inputAvailability(settings, [])).toEqual({
      canStart: false,
      activeLabel: "Studio Mic",
      warning: "No available input device is selected.",
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
