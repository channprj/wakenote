import { AudioLines } from "lucide-react";
import type { AppStatus } from "../lib/types";

const stateLabels: Record<AppStatus["tray_state"], string> = {
  idle: "Idle",
  listening: "Listening",
  recording: "Recording",
  transcribing: "Transcribing",
  paused: "Paused",
  error: "Error",
};

function levelPercent(dbfs: number) {
  return Math.max(0, Math.min(100, ((dbfs + 90) / 80) * 100));
}

export function FloatingOverlay({ status }: { status: AppStatus }) {
  const level = levelPercent(status.level.current_dbfs);
  const peak = levelPercent(status.level.peak_dbfs);
  const bars = Array.from({ length: 12 }, (_, index) => {
    const threshold = ((index + 1) / 12) * 100;
    return (
      <span
        key={index}
        data-lit={level >= threshold}
        style={{ height: `${24 + ((index % 4) + 1) * 14}%` }}
      />
    );
  });

  return (
    <aside
      className="floating-overlay"
      data-state={status.tray_state}
      aria-live="polite"
      role="status"
    >
      <div className="floating-overlay__mark">
        <AudioLines aria-hidden="true" />
      </div>
      <div className="floating-overlay__body">
        <strong>{stateLabels[status.tray_state]}</strong>
        <span>{Math.round(status.level.current_dbfs)} dBFS</span>
        <div className="floating-overlay__bars">{bars}</div>
        <i aria-hidden="true" style={{ width: `${peak}%` }} />
      </div>
    </aside>
  );
}
