import { describe, expect, it } from "vitest";
import { calibrationProgress, suggestedThresholdValue } from "./calibration";

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
});
