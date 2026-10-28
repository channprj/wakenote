import { describe, expect, it } from "vitest";
import {
  calibrationBadgePresentation,
  calibrationProgress,
  calibrationSettingsPatch,
  calibrationShouldAutoApply,
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

});

describe("calibrationBadgePresentation", () => {
  it("presents the calibration Badge for every calibration phase", () => {
    const cases: Array<[Parameters<typeof calibrationBadgePresentation>, ReturnType<typeof calibrationBadgePresentation>]> = [
      // Idle before calibration starts.
      [[null, 0, false], { tone: "neutral", label: "Calibration" }],
      // Progress in flight.
      [[1000, 0, false], { tone: "primary", label: "Calibrating" }],
      [[1000, 50, false], { tone: "primary", label: "Calibrating" }],
      [[1000, 99, false], { tone: "primary", label: "Calibrating" }],
      // Ready at 100% before auto-apply finalizes.
      [[1000, 100, false], { tone: "success", label: "Ready" }],
      // Applied once the auto-apply has run.
      [[1000, 100, true], { tone: "success", label: "Applied" }],
      // Defensive: even at percent < 100 an applied=true state should land on "Applied",
      // not "Calibrating" — protects against a hypothetical state shape where the parent
      // sets applied early.
      [[1000, 30, true], { tone: "success", label: "Applied" }],
    ];

    for (const [args, expected] of cases) {
      expect(calibrationBadgePresentation(...args), JSON.stringify(args)).toEqual(expected);
    }
  });
});
