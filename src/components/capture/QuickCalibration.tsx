import { RotateCcwIcon, TimerResetIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { StatusBadge } from "@/components/ui/status-badge";
import {
  calibrationBadgePresentation,
  calibrationProgress,
  calibrationShouldAutoApply,
  suggestedThresholdValue,
} from "@/lib/calibration";
import type { AppSettings, AppStatus } from "@/lib/types";

const CALIBRATION_DURATION_MS = 10_000;

export function QuickCalibration({
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
  const [calibrationStartMs, setCalibrationStartMs] = useState<number | null>(null);
  const [calibrationApplied, setCalibrationApplied] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const noiseFloor = active ? status.level.noise_floor_dbfs : -120;
  const suggestedThreshold = suggestedThresholdValue(
    active ? status.level.suggested_threshold_dbfs : -90,
  );
  const calibrationPercent =
    calibrationStartMs === null
      ? 0
      : calibrationProgress(calibrationStartMs, nowMs, CALIBRATION_DURATION_MS);
  const calibrationRunning = calibrationStartMs !== null && calibrationPercent < 100;
  const calibrationBadge = calibrationBadgePresentation(
    calibrationStartMs,
    calibrationPercent,
    calibrationApplied,
  );

  useEffect(() => {
    if (!calibrationRunning) {
      return;
    }
    const timer = window.setInterval(() => setNowMs(Date.now()), 250);
    return () => window.clearInterval(timer);
  }, [calibrationRunning]);

  useEffect(() => {
    if (
      !calibrationShouldAutoApply(
        calibrationStartMs,
        nowMs,
        CALIBRATION_DURATION_MS,
        calibrationApplied,
      )
    ) {
      return;
    }
    onApplyThreshold(suggestedThreshold);
    setCalibrationApplied(true);
  }, [
    calibrationApplied,
    calibrationStartMs,
    nowMs,
    onApplyThreshold,
    suggestedThreshold,
  ]);

  return (
    <Card size="sm" data-slot="quick-calibration" className="quick-calibration">
      <CardHeader>
        <CardTitle>Quick calibration</CardTitle>
        <CardDescription>Measure ambient noise for a stable voice trigger.</CardDescription>
        <CardAction>
          <StatusBadge tone={calibrationBadge.tone}>{calibrationBadge.label}</StatusBadge>
        </CardAction>
      </CardHeader>
      <CardContent>
        <div className="quick-calibration__metrics">
          <span>
            Noise floor <strong>{Math.round(noiseFloor)} dBFS</strong>
          </span>
          <span>
            Suggested <strong>{suggestedThreshold} dBFS</strong>
          </span>
          <span>
            Threshold <strong>{settings.threshold_dbfs} dBFS</strong>
          </span>
        </div>
        <Progress value={calibrationPercent} aria-label="Calibration progress" />
        <div className="quick-calibration__actions">
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={!active}
            title={!active ? "Start input to calibrate" : undefined}
            onClick={() => {
              const startedAt = Date.now();
              setCalibrationStartMs(startedAt);
              setNowMs(startedAt);
              setCalibrationApplied(false);
            }}
          >
            <TimerResetIcon data-icon="inline-start" />
            Calibrate
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={() => {
              setCalibrationStartMs(null);
              setCalibrationApplied(false);
              onResetRecordingSettings();
            }}
          >
            <RotateCcwIcon data-icon="inline-start" />
            Reset
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
