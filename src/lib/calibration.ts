import type { SettingsPatch } from "./types";

export function calibrationProgress(startMs: number, nowMs: number, durationMs: number) {
  if (durationMs <= 0) {
    return 100;
  }

  const elapsed = Math.max(0, nowMs - startMs);
  return Math.round(Math.min(100, (elapsed / durationMs) * 100));
}

export function suggestedThresholdValue(suggestedDbfs: number) {
  return Math.round(Math.max(-90, Math.min(-10, suggestedDbfs)));
}

export function calibrationSettingsPatch(thresholdDbfs: number): SettingsPatch {
  return {
    threshold_dbfs: thresholdDbfs,
    calibration_completed: true,
  };
}
