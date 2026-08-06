import { describe, expect, it } from "vitest";
import {
  RECORDING_DEFAULTS,
  resetRecordingSettingsPatch,
} from "./recording-settings";

const expectedDefaults = {
  threshold_dbfs: -40,
  attack_ms: 200,
  release_ms: 1_000,
  pre_roll_ms: 400,
  lead_in_padding_ms: 200,
  post_roll_ms: 400,
  min_chunk_ms: 800,
  max_chunk_ms: 180_000,
};

describe("recording settings", () => {
  it("defines the requested first-install recording profile", () => {
    expect(RECORDING_DEFAULTS).toEqual(expectedDefaults);
  });

  it("resets only the recording profile and calibration state", () => {
    const patch = resetRecordingSettingsPatch();

    expect(patch).toEqual({
      ...expectedDefaults,
      calibration_completed: false,
    });
    expect(patch).not.toHaveProperty("selected_microphone");
    expect(patch).not.toHaveProperty("input_monitoring_enabled");
    expect(patch).not.toHaveProperty("recording_enabled");
  });
});
