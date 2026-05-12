import { describe, expect, it } from "vitest";
import {
  DEFAULT_THRESHOLD_DBFS,
  calibrationBadgePresentation,
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

describe("calibrationBadgePresentation", () => {
  it("renders an idle Calibration Badge before calibration starts", () => {
    expect(calibrationBadgePresentation(null, 0, false)).toEqual({
      tone: "neutral",
      label: "Calibration",
    });
  });

  it("renders a Calibrating Badge while progress is in flight", () => {
    expect(calibrationBadgePresentation(1000, 0, false)).toEqual({
      tone: "primary",
      label: "Calibrating",
    });
    expect(calibrationBadgePresentation(1000, 50, false)).toEqual({
      tone: "primary",
      label: "Calibrating",
    });
    expect(calibrationBadgePresentation(1000, 99, false)).toEqual({
      tone: "primary",
      label: "Calibrating",
    });
  });

  it("renders a Ready Badge at 100% before auto-apply finalizes", () => {
    expect(calibrationBadgePresentation(1000, 100, false)).toEqual({
      tone: "success",
      label: "Ready",
    });
  });

  it("renders an Applied Badge once the auto-apply has run", () => {
    expect(calibrationBadgePresentation(1000, 100, true)).toEqual({
      tone: "success",
      label: "Applied",
    });
  });

  it("favors the applied label over the percent-driven branches", () => {
    // Defensive: even at percent < 100 an applied=true state should land on "Applied",
    // not "Calibrating" — protects against a hypothetical state shape where the parent
    // sets applied early.
    expect(calibrationBadgePresentation(1000, 30, true)).toEqual({
      tone: "success",
      label: "Applied",
    });
  });

  it("collapses Ready and Applied to success tone but distinguishes label", () => {
    const ready = calibrationBadgePresentation(1000, 100, false);
    const applied = calibrationBadgePresentation(1000, 100, true);
    expect(ready.tone).toBe(applied.tone);
    expect(ready.label).not.toBe(applied.label);
  });
});
