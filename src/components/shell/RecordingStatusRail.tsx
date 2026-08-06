import { AudioWaveformIcon, CircleStopIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/status-badge";
import type { DictationStage } from "@/lib/types";

export function RecordingStatusRail({
  liveActive,
  latestText,
  dictationState = "idle",
  onCancelDictation,
  onReturnToCapture,
}: {
  liveActive: boolean;
  latestText: string;
  dictationState?: DictationStage;
  onCancelDictation?: () => void;
  onReturnToCapture: () => void;
}) {
  const normalizedText = latestText.trim();
  const dictationActive =
    dictationState === "recording" || dictationState === "transcribing";
  if (!dictationActive && !liveActive && !normalizedText) {
    return null;
  }

  if (dictationActive) {
    const transcribing = dictationState === "transcribing";
    return (
      <aside
        data-slot="recording-status-rail"
        data-dictation={dictationState}
        className="recording-status-rail"
      >
        <StatusBadge tone="warning">
          <AudioWaveformIcon data-icon="inline-start" />
          {transcribing ? "Dictation transcribing" : "Dictation recording"}
        </StatusBadge>
        <span>{transcribing ? "Processing speech…" : "Listening…"}</span>
        <Button
          type="button"
          size="sm"
          variant="outline"
          className="shrink-0"
          onClick={onCancelDictation}
        >
          <CircleStopIcon data-icon="inline-start" />
          Cancel Dictation
        </Button>
      </aside>
    );
  }

  return (
    <aside
      data-slot="recording-status-rail"
      data-live={liveActive}
      className="recording-status-rail"
    >
      <StatusBadge tone={liveActive ? "success" : "neutral"}>
        <AudioWaveformIcon data-icon="inline-start" />
        {liveActive ? "Input live" : "Latest"}
      </StatusBadge>
      <span
        className="min-w-0 truncate"
        title={normalizedText || "Waiting for speech"}
      >
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
