import { Activity, Gauge } from "lucide-react";
import { Badge } from "./ui/primitives";
import type { AppSettings, AppStatus } from "../lib/types";

function meterPosition(dbfs: number) {
  return `${Math.max(0, Math.min(100, ((dbfs + 90) / 90) * 100))}%`;
}

export function LevelMeter({
  settings,
  status,
}: {
  settings: AppSettings;
  status: AppStatus;
}) {
  const active = settings.recording_enabled && !settings.pause_all;
  const currentDb = active ? status.level.current_dbfs : -120;
  const peakDb = status.level.peak_dbfs;
  const noiseFloor = status.level.noise_floor_dbfs;
  const suggestedThreshold = status.level.suggested_threshold_dbfs;
  const bars = Array.from({ length: 28 }, (_, index) => {
    const barDb = -90 + index * 3.2;
    return barDb <= currentDb;
  });

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
        <Badge tone={active ? "success" : "neutral"}>{status.tray_state}</Badge>
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
          <strong>{Math.round(suggestedThreshold)} dBFS</strong>
        </div>
        <div>
          <span>Threshold</span>
          <strong>{settings.threshold_dbfs} dBFS</strong>
        </div>
      </div>
    </div>
  );
}
