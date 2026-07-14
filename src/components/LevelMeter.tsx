import { Activity, Gauge, RotateCcw, TimerReset } from "lucide-react";
import { useEffect, useState } from "react";
import {
  calibrationBadgePresentation,
  calibrationProgress,
  calibrationShouldAutoApply,
  suggestedThresholdValue,
} from "../lib/calibration";
import { trayStateBadgeTone } from "../lib/status-summary";
import { humanizeTrayState } from "../lib/transcript-history";
import { Button } from "./ui/button";
import { Progress } from "./ui/progress";
import { StatusBadge } from "./ui/status-badge";
import type { AppSettings, AppStatus } from "../lib/types";

function meterPosition(dbfs: number) {
  return `${Math.max(0, Math.min(100, ((dbfs + 90) / 90) * 100))}%`;
}

export function calibrateDisabledReason(
  status: Pick<AppStatus, "live_input_active">,
): string | null {
  if (!status.live_input_active) return "Start input to calibrate";
  return null;
}

export function LevelMeter({
  settings,
  status,
  onApplyThreshold,
  onResetRecordingSettings,
}: {
  settings: AppSettings;
  status: AppStatus;
  onApplyThreshold: (thresholdDbfs: number) => void;
  onResetRecordingSettings: () => void;
}) {
  const active = status.live_input_active;
  const calibrateReason = calibrateDisabledReason(status);
  const [calibrationStartMs, setCalibrationStartMs] = useState<number | null>(null);
  const [calibrationApplied, setCalibrationApplied] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const currentDb = active ? status.level.current_dbfs : -120;
  const peakDb = active ? status.level.peak_dbfs : -120;
  const noiseFloor = active ? status.level.noise_floor_dbfs : -120;
  const suggestedThreshold = active ? status.level.suggested_threshold_dbfs : -90;
  const thresholdToApply = suggestedThresholdValue(suggestedThreshold);
  const calibrationPercent =
    calibrationStartMs === null ? 0 : calibrationProgress(calibrationStartMs, nowMs, 10_000);
  const calibrationRunning = calibrationStartMs !== null && calibrationPercent < 100;
  const calibrationBadge = calibrationBadgePresentation(
    calibrationStartMs,
    calibrationPercent,
    calibrationApplied,
  );
  const bars = Array.from({ length: 28 }, (_, index) => {
    const barDb = -90 + index * 3.2;
    return barDb <= currentDb;
  });

  useEffect(() => {
    if (!calibrationRunning) {
      return;
    }

    const timer = window.setInterval(() => setNowMs(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [calibrationRunning]);

  useEffect(() => {
    if (!calibrationShouldAutoApply(calibrationStartMs, nowMs, 10_000, calibrationApplied)) {
      return;
    }

    onApplyThreshold(thresholdToApply);
    setCalibrationApplied(true);
  }, [calibrationApplied, calibrationStartMs, nowMs, onApplyThreshold, thresholdToApply]);

  return (
    <div className="level-meter">
      <div className="level-meter__header">
        <h3>Calibration</h3>
        <div className="level-meter__header-actions">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => {
              const startedAt = Date.now();
              setCalibrationStartMs(startedAt);
              setNowMs(startedAt);
              setCalibrationApplied(false);
            }}
            disabled={!active}
            title={calibrateReason ?? undefined}
          >
            <TimerReset data-icon="inline-start" />
            Calibrate
          </Button>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => {
              setCalibrationStartMs(null);
              setCalibrationApplied(false);
              onResetRecordingSettings();
            }}
          >
            <RotateCcw data-icon="inline-start" />
            Reset
          </Button>
        </div>
      </div>
      <div className="level-meter__readout">
        <div>
          <span>Current</span>
          <strong>{Math.round(currentDb)} dBFS</strong>
        </div>
        <div>
          <span>Peak</span>
          <strong>{Math.round(peakDb)} dBFS</strong>
        </div>
        <div>
          <span>State</span>
          <strong>
            <StatusBadge tone={trayStateBadgeTone(status.tray_state)}>{humanizeTrayState(status.tray_state)}</StatusBadge>
          </strong>
        </div>
      </div>
      <div className="level-meter__track" aria-label="Microphone dBFS level">
        <i style={{ left: meterPosition(settings.threshold_dbfs) }} />
        {bars.map((lit, index) => (
          <span key={index} data-lit={lit} />
        ))}
      </div>
      <div className="calibration-grid">
        <div>
          <Gauge />
          <span>Noise floor</span>
          <strong>{Math.round(noiseFloor)} dBFS</strong>
        </div>
        <div>
          <Activity />
          <span>Suggested</span>
          <strong>{thresholdToApply} dBFS</strong>
        </div>
        <div>
          <span>Threshold</span>
          <strong>{settings.threshold_dbfs} dBFS</strong>
        </div>
      </div>
      <div className="calibration-status">
        <StatusBadge tone={calibrationBadge.tone}>{calibrationBadge.label}</StatusBadge>
        <Progress value={calibrationPercent} />
      </div>
    </div>
  );
}
