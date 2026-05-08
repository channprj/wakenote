import { Activity, Check, Gauge, TimerReset } from "lucide-react";
import { useEffect, useState } from "react";
import { calibrationProgress, suggestedThresholdValue } from "../lib/calibration";
import { Badge, Button, Progress } from "./ui/primitives";
import type { AppSettings, AppStatus } from "../lib/types";

function meterPosition(dbfs: number) {
  return `${Math.max(0, Math.min(100, ((dbfs + 90) / 90) * 100))}%`;
}

export function LevelMeter({
  settings,
  status,
  onApplyThreshold,
}: {
  settings: AppSettings;
  status: AppStatus;
  onApplyThreshold: (thresholdDbfs: number) => void;
}) {
  const active = status.live_input_active;
  const [calibrationStartMs, setCalibrationStartMs] = useState<number | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const currentDb = active ? status.level.current_dbfs : -120;
  const peakDb = active ? status.level.peak_dbfs : -120;
  const noiseFloor = active ? status.level.noise_floor_dbfs : -120;
  const suggestedThreshold = active ? status.level.suggested_threshold_dbfs : -90;
  const thresholdToApply = suggestedThresholdValue(suggestedThreshold);
  const calibrationPercent =
    calibrationStartMs === null ? 0 : calibrationProgress(calibrationStartMs, nowMs, 10_000);
  const calibrationRunning = calibrationStartMs !== null && calibrationPercent < 100;
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

  return (
    <div className="level-meter">
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
            <Badge tone={active ? "success" : "neutral"}>{status.tray_state}</Badge>
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
      <div className="calibration-actions">
        <Badge
          tone={
            calibrationStartMs === null ? "neutral" : calibrationPercent >= 100 ? "success" : "primary"
          }
        >
          {calibrationStartMs === null
            ? "Calibration"
            : calibrationPercent >= 100
              ? "Ready"
              : "Calibrating"}
        </Badge>
        <Progress value={calibrationPercent} />
        <Button
          type="button"
          variant="secondary"
          size="sm"
          onClick={() => {
            setCalibrationStartMs(Date.now());
            setNowMs(Date.now());
          }}
          disabled={!active}
        >
          <TimerReset data-icon="inline-start" />
          Calibrate
        </Button>
        <Button
          type="button"
          variant="primary"
          size="sm"
          onClick={() => onApplyThreshold(thresholdToApply)}
          disabled={!active}
        >
          <Check data-icon="inline-start" />
          Apply
        </Button>
      </div>
    </div>
  );
}
