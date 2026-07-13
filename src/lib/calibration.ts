import type { StatusTone } from "./status-summary";
import type { SettingsPatch } from "./types";

export interface CalibrationBadgePresentation {
  tone: StatusTone;
  label: string;
}

// Folds LevelMeter's calibration Badge tone+label dual-conditional into one shared helper.
// State transitions: startMs === null → idle (neutral / "Calibration"); applied → success /
// "Applied"; percent >= 100 → success / "Ready"; otherwise → primary / "Calibrating". The
// tone deliberately collapses "Applied" and "Ready" to success because both are end-states
// from the user's POV, while the label distinguishes them — keeping both in one helper
// prevents the previous inline pattern's drift risk where one conditional could update
// without the other.
export function calibrationBadgePresentation(
  startMs: number | null,
  percent: number,
  applied: boolean,
): CalibrationBadgePresentation {
  if (startMs === null) return { tone: "neutral", label: "Calibration" };
  if (applied) return { tone: "success", label: "Applied" };
  if (percent >= 100) return { tone: "success", label: "Ready" };
  return { tone: "primary", label: "Calibrating" };
}

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

export function calibrationShouldAutoApply(
  startMs: number | null,
  nowMs: number,
  durationMs: number,
  alreadyApplied: boolean,
) {
  if (startMs === null || alreadyApplied) {
    return false;
  }
  if (durationMs <= 0) {
    return true;
  }

  return Math.max(0, nowMs - startMs) >= durationMs;
}

export function calibrationSettingsPatch(thresholdDbfs: number): SettingsPatch {
  return {
    threshold_dbfs: thresholdDbfs,
    calibration_completed: true,
  };
}
