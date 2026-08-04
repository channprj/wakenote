import { XIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import {
  audioPlaybackUrlFromPath,
  initialAudioPlaybackSource,
} from "@/lib/audio-playback";
import {
  audioPathBasename,
  humanizeQueueJobStatus,
} from "@/lib/transcript-history";
import type { QueueJob } from "@/lib/types";

export function ActivityAudioPlayer({
  job,
  onClose,
}: {
  job: QueueJob;
  onClose: () => void;
}) {
  const [audioSource, setAudioSource] = useState(() =>
    initialAudioPlaybackSource(job.audio_path),
  );
  const [audioError, setAudioError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setAudioSource(initialAudioPlaybackSource(job.audio_path));
    setAudioError(null);
    audioPlaybackUrlFromPath(job.audio_path)
      .then((url) => {
        if (!cancelled) {
          setAudioSource(url);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setAudioError(
            error instanceof Error ? error.message : "Could not prepare recording",
          );
        }
      });
    return () => {
      cancelled = true;
    };
  }, [job.audio_path]);

  return (
    <aside
      aria-label="Activity audio player"
      className="transcript-player-dock activity-audio-player"
      data-slot="activity-audio-player"
    >
      <div className="transcript-player-dock__copy">
        <small>Now playing</small>
        <strong title={job.audio_path}>
          {audioPathBasename(job.audio_path)}
        </strong>
        <span>
          Activity job #{job.id} · {humanizeQueueJobStatus(job.status)}
        </span>
      </div>
      <audio
        autoPlay
        controls
        key={audioSource || "pending"}
        preload="metadata"
        src={audioSource || undefined}
      />
      {audioError ? (
        <span className="transcript-player-dock__error" role="alert">
          {audioError}
        </span>
      ) : null}
      <Button
        aria-label="Close Activity audio player"
        onClick={onClose}
        size="icon-sm"
        type="button"
        variant="ghost"
      >
        <XIcon data-icon="solo" />
      </Button>
    </aside>
  );
}
