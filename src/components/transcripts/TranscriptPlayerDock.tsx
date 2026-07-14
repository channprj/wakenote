import { PauseIcon, PlayIcon, XIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/status-badge";
import { audioPlaybackUrlFromPath, initialAudioPlaybackSource } from "@/lib/audio-playback";
import { fileUrlFromPath, formatLocalTimestamp } from "@/lib/transcript-history";
import type { RecentTranscript } from "@/lib/types";

export interface TranscriptPlayerDockProps {
  entry: RecentTranscript;
  paused: boolean;
  autoPlayNext: boolean;
  onClose: () => void;
  onPausedChange: (paused: boolean) => void;
  onEnded: () => void;
}

export function TranscriptPlayerDock({
  entry,
  paused,
  autoPlayNext,
  onClose,
  onPausedChange,
  onEnded,
}: TranscriptPlayerDockProps) {
  const audioPath = entry.audio_path;
  const [audioSource, setAudioSource] = useState(() =>
    initialAudioPlaybackSource(audioPath),
  );
  const [audioError, setAudioError] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);

  useEffect(() => {
    if (!audioPath) return;
    let cancelled = false;
    setAudioSource(initialAudioPlaybackSource(audioPath));
    setAudioError(null);
    audioPlaybackUrlFromPath(audioPath)
      .then((url) => {
        if (!cancelled) setAudioSource(url);
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
  }, [audioPath]);

  useEffect(() => {
    const element = audioRef.current;
    if (!element) return;
    const handlePlay = () => onPausedChange(false);
    const handlePause = () => onPausedChange(true);
    element.addEventListener("play", handlePlay);
    element.addEventListener("pause", handlePause);
    element.addEventListener("ended", onEnded);
    return () => {
      element.removeEventListener("play", handlePlay);
      element.removeEventListener("pause", handlePause);
      element.removeEventListener("ended", onEnded);
    };
  }, [audioSource, onPausedChange, onEnded]);

  useEffect(() => {
    const element = audioRef.current;
    if (!element || !audioSource) return;
    if (paused) {
      element.pause();
    } else {
      void element.play().catch(() => undefined);
    }
  }, [paused, audioSource]);

  if (!audioPath) return null;

  const timestamp = formatLocalTimestamp(entry.recorded_at);
  return (
    <aside
      data-slot="transcript-player-dock"
      className="transcript-player-dock"
      aria-label="Transcript player"
    >
      <Button
        type="button"
        size="icon-sm"
        variant="outline"
        aria-label={paused ? "Play recording" : "Pause recording"}
        onClick={() => onPausedChange(!paused)}
      >
        {paused ? <PlayIcon data-icon="solo" /> : <PauseIcon data-icon="solo" />}
      </Button>
      <div className="transcript-player-dock__copy">
        <small>Now playing</small>
        <a href={fileUrlFromPath(audioPath)} title={audioPath}>
          {timestamp || audioPath}
        </a>
        <span title={entry.text}>{entry.text}</span>
      </div>
      <StatusBadge tone={autoPlayNext ? "success" : "neutral"}>
        Autoplay next {autoPlayNext ? "on" : "off"}
      </StatusBadge>
      <audio
        autoPlay={!paused}
        controls
        key={audioSource || "pending"}
        preload="metadata"
        ref={audioRef}
        src={audioSource || undefined}
      />
      {audioError ? (
        <span className="transcript-player-dock__error" role="alert">{audioError}</span>
      ) : null}
      <Button
        type="button"
        size="icon-sm"
        variant="ghost"
        aria-label="Close player"
        onClick={onClose}
      >
        <XIcon data-icon="solo" />
      </Button>
    </aside>
  );
}
