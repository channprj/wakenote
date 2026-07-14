import { AudioWaveformIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/status-badge";

export function RecordingStatusRail({
  liveActive,
  latestText,
  onReturnToCapture,
}: {
  liveActive: boolean;
  latestText: string;
  onReturnToCapture: () => void;
}) {
  const normalizedText = latestText.trim();
  if (!liveActive && !normalizedText) {
    return null;
  }

  return (
    <aside data-slot="recording-status-rail" data-live={liveActive} className="recording-status-rail">
      <StatusBadge tone={liveActive ? "success" : "neutral"}>
        <AudioWaveformIcon data-icon="inline-start" />
        {liveActive ? "Input live" : "Latest"}
      </StatusBadge>
      <span className="min-w-0 truncate" title={normalizedText || "Waiting for speech"}>
        {normalizedText || "Waiting for speech"}
      </span>
      <Button
        type="button"
        size="sm"
        variant="ghost"
        className="shrink-0"
        onClick={onReturnToCapture}
      >
        Return to Capture
      </Button>
    </aside>
  );
}
