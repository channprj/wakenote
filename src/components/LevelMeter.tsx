import { Activity, Gauge } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
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
  const [tick, setTick] = useState(0);
  const active = status.tray_state === "listening" || status.tray_state === "recording";

  useEffect(() => {
    const timer = window.setInterval(() => setTick((value) => value + 1), 100);
    return () => window.clearInterval(timer);
  }, []);

  const currentDb = useMemo(() => {
    if (!active) {
      return -120;
    }
    const speechPulse = Math.sin(tick / 4) * 12;
    const room = Math.sin(tick / 11) * 5;
    return Math.round(-52 + speechPulse + room);
  }, [active, tick]);
  const peakDb = Math.max(currentDb, settings.threshold_dbfs + 4);
  const noiseFloor = -62;
  const suggestedThreshold = noiseFloor + 12;
  const bars = Array.from({ length: 28 }, (_, index) => {
    const barDb = -90 + index * 3.2;
    return barDb <= currentDb;
  });

  return (
    <div className="level-meter">
      <div className="level-meter__readout">
        <div>
          <span>Current</span>
          <strong>{currentDb} dBFS</strong>
        </div>
        <div>
          <span>Peak</span>
          <strong>{peakDb} dBFS</strong>
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
          <strong>{noiseFloor} dBFS</strong>
        </div>
        <div>
          <Activity />
          <span>Suggested</span>
          <strong>{suggestedThreshold} dBFS</strong>
        </div>
        <div>
          <span>Threshold</span>
          <strong>{settings.threshold_dbfs} dBFS</strong>
        </div>
      </div>
    </div>
  );
}
