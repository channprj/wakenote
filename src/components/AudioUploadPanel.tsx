import { FileAudio, FolderOpen, Loader2, Upload } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { audioPlaybackUrlFromPath } from "../lib/audio-playback";
import { nextAudibleTime } from "../lib/audio-player";
import {
  chooseUploadedAudioFile,
  loadAudioWaveform,
} from "../lib/tauri-client";
import { fileUrlFromPath } from "../lib/transcript-history";
import type { AudioWaveform, UploadedAudio } from "../lib/types";
import { displayWaveformPeaks, visibleWaveformPeaks } from "../lib/waveform";
import { Badge, Button, Slider, Switch } from "./ui/primitives";

export function AudioUploadPanel({
  initialAudio = null,
  initialWaveform = null,
}: {
  initialAudio?: UploadedAudio | null;
  initialWaveform?: AudioWaveform | null;
}) {
  const [audio, setAudio] = useState<UploadedAudio | null>(initialAudio);
  const [waveform, setWaveform] = useState<AudioWaveform | null>(initialWaveform);
  const [audioSource, setAudioSource] = useState(
    initialAudio ? fileUrlFromPath(initialAudio.audio_path) : "",
  );
  const [zoom, setZoom] = useState(1);
  const [centerRatio, setCenterRatio] = useState(0.5);
  const [skipSilence, setSkipSilence] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const initialAudioPath = initialAudio?.audio_path ?? null;

  const visiblePeaks = useMemo(
    () => visibleWaveformPeaks(waveform?.peaks ?? [], zoom, centerRatio),
    [waveform?.peaks, zoom, centerRatio],
  );

  useEffect(() => {
    if (!audio) {
      setAudioSource("");
      return;
    }

    let cancelled = false;
    const fallbackSource = fileUrlFromPath(audio.audio_path);
    setAudioSource(fallbackSource);
    audioPlaybackUrlFromPath(audio.audio_path)
      .then((source) => {
        if (!cancelled) {
          setAudioSource(source);
        }
      })
      .catch((caught: unknown) => {
        if (!cancelled) {
          setError(caught instanceof Error ? caught.message : "Could not prepare audio playback");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [audio?.audio_path]);

  useEffect(() => {
    if (!audio || (initialWaveform && audio.audio_path === initialAudioPath)) {
      return;
    }

    let cancelled = false;
    setWaveform(null);
    setError(null);
    void loadAudioWaveform(audio.audio_path)
      .then((nextWaveform) => {
        if (!cancelled) {
          setWaveform(nextWaveform);
        }
      })
      .catch((caught: unknown) => {
        if (!cancelled) {
          setError(caught instanceof Error ? caught.message : "Could not analyze audio waveform");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [audio?.audio_path, initialAudioPath, initialWaveform]);

  useEffect(() => {
    const element = audioRef.current;
    if (!element || !waveform || !skipSilence) {
      return;
    }

    const skipCurrentSilence = () => {
      const next = nextAudibleTime(element.currentTime, waveform.audible_ranges);
      if (next !== null && next > element.currentTime) {
        element.currentTime = next;
      }
    };

    element.addEventListener("play", skipCurrentSilence);
    element.addEventListener("timeupdate", skipCurrentSilence);
    return () => {
      element.removeEventListener("play", skipCurrentSilence);
      element.removeEventListener("timeupdate", skipCurrentSilence);
    };
  }, [skipSilence, waveform]);

  const handleUpload = async () => {
    setBusy(true);
    setError(null);
    try {
      const uploaded = await chooseUploadedAudioFile();
      if (uploaded) {
        setAudio(uploaded);
        setWaveform(null);
        setZoom(1);
        setCenterRatio(0.5);
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="audio-upload-panel">
      <div className="audio-upload-panel__toolbar">
        <Button type="button" variant="primary" onClick={handleUpload} disabled={busy}>
          {busy ? <Loader2 data-icon="inline-start" /> : <Upload data-icon="inline-start" />}
          Upload Audio
        </Button>
      </div>

      {error ? <div className="error-banner">{error}</div> : null}

      {audio ? (
        <div className="audio-workbench">
          <header className="audio-workbench__header">
            <div className="audio-workbench__title">
              <FileAudio />
              <div>
                <span>Uploaded audio</span>
                <a href={fileUrlFromPath(audio.audio_path)} title={audio.audio_path}>
                  <strong>{audio.original_filename || audio.audio_path}</strong>
                </a>
              </div>
            </div>
            <Badge tone={waveform ? "success" : "warning"}>
              {waveform ? formatDuration(waveform.duration_seconds) : "Analyzing"}
            </Badge>
          </header>

          {waveform ? (
            <>
              <WaveformView
                className="audio-waveform-overview"
                label="Full waveform"
                peaks={displayWaveformPeaks(waveform.peaks, 220)}
              />
              <div className="audio-workbench__controls">
                <Slider
                  label="Zoom"
                  min={1}
                  max={16}
                  step={1}
                  value={zoom}
                  suffix="x"
                  onValueChange={setZoom}
                />
                <Slider
                  label="Position"
                  min={0}
                  max={100}
                  value={Math.round(centerRatio * 100)}
                  suffix="%"
                  onValueChange={(value) => setCenterRatio(value / 100)}
                />
                <div className="audio-workbench__switch">
                  <span>Skip Silence</span>
                  <Switch
                    label="Skip Silence"
                    checked={skipSilence}
                    onCheckedChange={setSkipSilence}
                  />
                </div>
              </div>
              <WaveformView
                className="audio-waveform-detail"
                label="Zoomed waveform"
                peaks={displayWaveformPeaks(visiblePeaks, 320)}
              />
            </>
          ) : (
            <div className="audio-waveform-loading">
              <Loader2 />
              <span>Analyzing</span>
            </div>
          )}

          <div className="audio-player-row">
            <audio ref={audioRef} controls key={audioSource} preload="metadata" src={audioSource} />
            <a href={fileUrlFromPath(audio.audio_path)} title={audio.audio_path}>
              <FolderOpen />
              Open file
            </a>
          </div>
        </div>
      ) : (
        <div className="audio-upload-empty">
          <FileAudio />
          <span>No uploaded audio selected</span>
        </div>
      )}
    </div>
  );
}

function WaveformView({
  peaks,
  className,
  label,
}: {
  peaks: number[];
  className: string;
  label: string;
}) {
  return (
    <div className={className} role="img" aria-label={label}>
      {peaks.map((peak, index) => (
        <span
          key={`${index}-${peak}`}
          style={{ height: `${Math.max(4, Math.round(Math.min(1, peak) * 100))}%` }}
        />
      ))}
    </div>
  );
}

function formatDuration(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) {
    return "0:00";
  }

  const totalSeconds = Math.round(seconds);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const remainingSeconds = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(remainingSeconds).padStart(2, "0")}`;
  }

  return `${minutes}:${String(remainingSeconds).padStart(2, "0")}`;
}
