import {
  FileAudio,
  FolderOpen,
  Loader2,
  Pause,
  Play,
  Upload,
} from "lucide-react";
import {
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { audioPlaybackUrlFromPath } from "../lib/audio-playback";
import { nextAudibleTime } from "../lib/audio-player";
import {
  chooseUploadedAudioFile,
  loadAudioWaveform,
} from "../lib/tauri-client";
import { fileUrlFromPath } from "../lib/transcript-history";
import type { AudioWaveform, UploadedAudio } from "../lib/types";
import { displayWaveformPeaks } from "../lib/waveform";
import { Badge, Button, Slider, Switch } from "./ui/primitives";

const ZOOM_MIN = 1;
const ZOOM_MAX = 12;
const PIXELS_PER_SECOND_AT_ZOOM_1 = 60;
const SKIP_OFFSETS_SECONDS = [-10, -5, 5, 10] as const;
const OVERVIEW_BAR_COUNT = 220;
const WAVEFORM_HEIGHT = 180;

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
  const [zoom, setZoom] = useState(3);
  const [skipSilence, setSkipSilence] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(initialWaveform?.duration_seconds ?? 0);
  const [isPlaying, setIsPlaying] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const initialAudioPath = initialAudio?.audio_path ?? null;

  const pixelsPerSecond = zoom * PIXELS_PER_SECOND_AT_ZOOM_1;
  const overviewPeaks = useMemo(
    () => displayWaveformPeaks(waveform?.peaks ?? [], OVERVIEW_BAR_COUNT),
    [waveform?.peaks],
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
          setError(
            caught instanceof Error
              ? caught.message
              : "Could not prepare audio playback",
          );
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
          setError(
            caught instanceof Error
              ? caught.message
              : "Could not analyze audio waveform",
          );
        }
      });

    return () => {
      cancelled = true;
    };
  }, [audio?.audio_path, initialAudioPath, initialWaveform]);

  useEffect(() => {
    if (waveform) {
      setDuration(waveform.duration_seconds);
    }
  }, [waveform?.duration_seconds]);

  // Reset playback when audio source changes.
  useEffect(() => {
    setCurrentTime(0);
    setIsPlaying(false);
  }, [audioSource]);

  // Bind audio element listeners for playback metadata.
  useEffect(() => {
    const element = audioRef.current;
    if (!element) {
      return;
    }

    const handlePlay = () => setIsPlaying(true);
    const handlePause = () => setIsPlaying(false);
    const handleEnded = () => setIsPlaying(false);
    const handleLoadedMetadata = () => {
      if (Number.isFinite(element.duration) && element.duration > 0) {
        setDuration(element.duration);
      }
    };
    const handleTimeUpdate = () => setCurrentTime(element.currentTime);
    const handleSeeked = () => setCurrentTime(element.currentTime);

    element.addEventListener("play", handlePlay);
    element.addEventListener("pause", handlePause);
    element.addEventListener("ended", handleEnded);
    element.addEventListener("loadedmetadata", handleLoadedMetadata);
    element.addEventListener("timeupdate", handleTimeUpdate);
    element.addEventListener("seeked", handleSeeked);

    return () => {
      element.removeEventListener("play", handlePlay);
      element.removeEventListener("pause", handlePause);
      element.removeEventListener("ended", handleEnded);
      element.removeEventListener("loadedmetadata", handleLoadedMetadata);
      element.removeEventListener("timeupdate", handleTimeUpdate);
      element.removeEventListener("seeked", handleSeeked);
    };
  }, [audioSource]);

  // High-frequency playhead + skip-silence loop while playing. Drives the
  // scrolling waveform animation and forwards the playhead past silence the
  // moment it enters a gap, instead of waiting on `timeupdate` (~4 Hz).
  useEffect(() => {
    if (!isPlaying) {
      return;
    }
    const element = audioRef.current;
    if (!element) {
      return;
    }

    let raf = 0;
    const tick = () => {
      if (skipSilence && waveform) {
        const next = nextAudibleTime(element.currentTime, waveform.audible_ranges);
        if (next !== null && next > element.currentTime) {
          element.currentTime = next;
        }
      }
      setCurrentTime(element.currentTime);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [isPlaying, skipSilence, waveform]);

  const seek = useCallback(
    (time: number) => {
      const element = audioRef.current;
      const clamped = Math.max(0, Math.min(duration || Number.POSITIVE_INFINITY, time));
      if (element) {
        element.currentTime = clamped;
      }
      setCurrentTime(clamped);
    },
    [duration],
  );

  const togglePlay = useCallback(() => {
    const element = audioRef.current;
    if (!element) {
      return;
    }
    if (element.paused) {
      void element.play().catch(() => undefined);
    } else {
      element.pause();
    }
  }, []);

  const skipBy = useCallback(
    (offsetSeconds: number) => {
      const element = audioRef.current;
      const baseline = element ? element.currentTime : currentTime;
      seek(baseline + offsetSeconds);
    },
    [currentTime, seek],
  );

  const handleUpload = async () => {
    setBusy(true);
    setError(null);
    try {
      const uploaded = await chooseUploadedAudioFile();
      if (uploaded) {
        setAudio(uploaded);
        setWaveform(null);
        setZoom(3);
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
              <WaveformOverview
                peaks={overviewPeaks}
                duration={duration}
                currentTime={currentTime}
                onSeek={seek}
              />
              <WaveformDetail
                peaks={waveform.peaks}
                duration={duration}
                currentTime={currentTime}
                pixelsPerSecond={pixelsPerSecond}
                onSeek={seek}
              />
              <TransportControls
                isPlaying={isPlaying}
                currentTime={currentTime}
                duration={duration}
                onTogglePlay={togglePlay}
                onSkip={skipBy}
              />
              <div className="audio-workbench__controls">
                <Slider
                  label="Zoom"
                  min={ZOOM_MIN}
                  max={ZOOM_MAX}
                  step={1}
                  value={zoom}
                  suffix="x"
                  onValueChange={setZoom}
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
            </>
          ) : (
            <div className="audio-waveform-loading">
              <Loader2 />
              <span>Analyzing</span>
            </div>
          )}

          <div className="audio-player-row">
            <audio
              ref={audioRef}
              key={audioSource}
              preload="metadata"
              src={audioSource}
            />
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

function WaveformOverview({
  peaks,
  duration,
  currentTime,
  onSeek,
}: {
  peaks: number[];
  duration: number;
  currentTime: number;
  onSeek: (time: number) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const progressRatio = duration > 0 ? Math.max(0, Math.min(1, currentTime / duration)) : 0;

  const seekFromPointer = useCallback(
    (clientX: number) => {
      const element = containerRef.current;
      if (!element || duration <= 0) {
        return;
      }
      const rect = element.getBoundingClientRect();
      const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
      onSeek(ratio * duration);
    },
    [duration, onSeek],
  );

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) {
      return;
    }
    event.currentTarget.setPointerCapture(event.pointerId);
    seekFromPointer(event.clientX);
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.buttons !== 1) {
      return;
    }
    seekFromPointer(event.clientX);
  };

  return (
    <div
      ref={containerRef}
      className="audio-waveform-overview"
      role="img"
      aria-label="Full waveform"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
    >
      <div className="audio-waveform-overview__bars">
        {peaks.map((peak, index) => (
          <span
            key={`${index}-${peak}`}
            style={{ height: `${Math.max(4, Math.round(Math.min(1, peak) * 100))}%` }}
          />
        ))}
      </div>
      <span
        className="audio-waveform-overview__progress"
        style={{ width: `${progressRatio * 100}%` }}
      />
      <span
        className="audio-waveform-overview__cursor"
        style={{ left: `${progressRatio * 100}%` }}
      />
    </div>
  );
}

function WaveformDetail({
  peaks,
  duration,
  currentTime,
  pixelsPerSecond,
  onSeek,
}: {
  peaks: number[];
  duration: number;
  currentTime: number;
  pixelsPerSecond: number;
  onSeek: (time: number) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const dragStateRef = useRef<{ startX: number; startTime: number } | null>(null);
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const element = containerRef.current;
    if (!element || typeof ResizeObserver === "undefined") {
      return;
    }

    const update = () => {
      setWidth(Math.round(element.getBoundingClientRect().width));
    };
    update();

    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || width <= 0 || peaks.length === 0 || duration <= 0) {
      return;
    }

    const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
    const cssHeight = WAVEFORM_HEIGHT;
    canvas.width = Math.max(1, Math.round(width * dpr));
    canvas.height = Math.max(1, Math.round(cssHeight * dpr));
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, cssHeight);

    const centerX = width / 2;
    const barWidth = 2;
    const barGap = 1;
    const barStride = barWidth + barGap;
    const secondsPerPeak = duration / peaks.length;
    const peaksPerBar = Math.max(1, Math.ceil(barStride / pixelsPerSecond / secondsPerPeak));

    const startX = (Math.round(centerX) % barStride);
    for (let x = startX; x < width; x += barStride) {
      const t = currentTime + (x - centerX) / pixelsPerSecond;
      if (t < 0 || t > duration) {
        continue;
      }
      const startIndex = Math.floor(t / secondsPerPeak);
      const endIndex = Math.min(peaks.length, startIndex + peaksPerBar);
      let amp = 0;
      for (let i = startIndex; i < endIndex; i += 1) {
        if (peaks[i] > amp) {
          amp = peaks[i];
        }
      }
      const barHeight = Math.max(2, amp * cssHeight * 0.85);
      const y = (cssHeight - barHeight) / 2;
      ctx.fillStyle = t <= currentTime ? "#1e3a8a" : "#94a3b8";
      ctx.fillRect(x, y, barWidth, barHeight);
    }

    // Fixed red center cursor.
    ctx.fillStyle = "#dc2626";
    ctx.fillRect(Math.round(centerX) - 1, 0, 2, cssHeight);
  }, [width, peaks, duration, currentTime, pixelsPerSecond]);

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || duration <= 0) {
      return;
    }
    dragStateRef.current = { startX: event.clientX, startTime: currentTime };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const handlePointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const state = dragStateRef.current;
    if (!state || pixelsPerSecond <= 0) {
      return;
    }
    const dx = event.clientX - state.startX;
    const dt = -dx / pixelsPerSecond;
    onSeek(state.startTime + dt);
  };

  const handlePointerUp = (event: ReactPointerEvent<HTMLDivElement>) => {
    dragStateRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
  };

  return (
    <div
      ref={containerRef}
      className="audio-waveform-detail"
      role="img"
      aria-label="Zoomed waveform"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
    >
      <canvas ref={canvasRef} style={{ width: "100%", height: WAVEFORM_HEIGHT }} />
    </div>
  );
}

function TransportControls({
  isPlaying,
  currentTime,
  duration,
  onTogglePlay,
  onSkip,
}: {
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  onTogglePlay: () => void;
  onSkip: (offsetSeconds: number) => void;
}) {
  return (
    <div className="audio-transport">
      <div className="audio-transport__buttons">
        {SKIP_OFFSETS_SECONDS.filter((offset) => offset < 0).map((offset) => (
          <SkipButton key={offset} offset={offset} onSkip={onSkip} />
        ))}
        <button
          type="button"
          className="audio-transport__play"
          onClick={onTogglePlay}
          aria-label={isPlaying ? "Pause" : "Play"}
        >
          {isPlaying ? <Pause /> : <Play />}
        </button>
        {SKIP_OFFSETS_SECONDS.filter((offset) => offset > 0).map((offset) => (
          <SkipButton key={offset} offset={offset} onSkip={onSkip} />
        ))}
      </div>
      <div className="audio-transport__time" aria-live="polite">
        <span className="audio-transport__time-current">
          {formatTime(currentTime)}
        </span>
        <span className="audio-transport__time-separator">/</span>
        <span className="audio-transport__time-total">{formatTime(duration)}</span>
      </div>
    </div>
  );
}

function SkipButton({
  offset,
  onSkip,
}: {
  offset: number;
  onSkip: (offset: number) => void;
}) {
  const absSeconds = Math.abs(offset);
  const label = offset < 0 ? `Skip back ${absSeconds} seconds` : `Skip forward ${absSeconds} seconds`;
  return (
    <button
      type="button"
      className={`audio-transport__skip audio-transport__skip--${offset < 0 ? "back" : "fwd"}`}
      onClick={() => onSkip(offset)}
      aria-label={label}
    >
      <span className="audio-transport__skip-glyph" aria-hidden>
        {offset < 0 ? "⟲" : "⟳"}
      </span>
      <span className="audio-transport__skip-amount">{absSeconds}</span>
    </button>
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

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds < 0) {
    return "0:00";
  }
  return formatDuration(seconds);
}
