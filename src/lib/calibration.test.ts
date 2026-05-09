import { describe, expect, it } from "vitest";
import {
  DEFAULT_THRESHOLD_DBFS,
  calibrationProgress,
  calibrationSettingsPatch,
  calibrationShouldAutoApply,
  resetCalibrationSettingsPatch,
  suggestedThresholdValue,
} from "./calibration";

describe("calibration helpers", () => {
  it("reports bounded calibration progress", () => {
    expect(calibrationProgress(1000, 1000, 5000)).toBe(0);
    expect(calibrationProgress(1000, 3500, 5000)).toBe(50);
    expect(calibrationProgress(1000, 9000, 5000)).toBe(100);
  });

  it("rounds suggested thresholds to valid dBFS values", () => {
    expect(suggestedThresholdValue(-50.4)).toBe(-50);
    expect(suggestedThresholdValue(-120)).toBe(-90);
    expect(suggestedThresholdValue(5)).toBe(-10);
  });

  it("marks setup calibration complete when applying the suggested threshold", () => {
    expect(calibrationSettingsPatch(-43)).toEqual({
      threshold_dbfs: -43,
      calibration_completed: true,
    });
  });

  it("auto-applies once calibration reaches completion", () => {
    expect(calibrationShouldAutoApply(1000, 10_999, 10_000, false)).toBe(false);
    expect(calibrationShouldAutoApply(1000, 11_000, 10_000, false)).toBe(true);
    expect(calibrationShouldAutoApply(1000, 12_000, 10_000, true)).toBe(false);
  });

  it("resets threshold calibration to the default value", () => {
    expect(DEFAULT_THRESHOLD_DBFS).toBe(-60);
    expect(resetCalibrationSettingsPatch()).toEqual({
      threshold_dbfs: -60,
      calibration_completed: false,
    });
  });
});
