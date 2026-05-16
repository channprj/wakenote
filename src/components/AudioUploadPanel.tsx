import {
  ChevronDown,
  FileAudio,
  FolderOpen,
  Loader2,
  Pause,
  Play,
  RotateCcw,
  RotateCw,
  Upload,
  Wand2,
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
import {
  amplitudeToDisplay,
  audibleRangesFromPeaks,
  dbfsToDisplay,
  nextAudibleTime,
} from "../lib/audio-player";
import {
  chooseUploadedAudioFile,
  loadAudioWaveform,
} from "../lib/tauri-client";
import { fileUrlFromPath } from "../lib/transcript-history";
import type { AudioWaveform, UploadedAudio } from "../lib/types";
import {
  type SignedWaveformView,
  displaySignedWaveformPeaks,
} from "../lib/waveform";
import { Badge, Button, Slider, Switch } from "./ui/primitives";

const ZOOM_MIN = 1;
const ZOOM_MAX = 12;
const PIXELS_PER_SECOND_AT_ZOOM_1 = 60;
const SKIP_OFFSETS_SECONDS = [-10, -5, 5, 10] as const;
const OVERVIEW_BAR_COUNT = 320;
const WAVEFORM_HEIGHT = 224;
const WAVEFORM_RULER_HEIGHT = 22;
const WAVEFORM_BAR_FILL_RATIO = 0.92;
const WAVEFORM_BUCKET_COUNT = 16384;
const THRESHOLD_DBFS_DEFAULT = -45;
const THRESHOLD_DBFS_MIN = -80;
const THRESHOLD_DBFS_MAX = 0;
const THRESHOLD_DBFS_STEP = 0.5;
const THRESHOLD_COLOR = "#f97316"; // orange-500 — the "기준선"
const PLAYED_COLOR = "#1e3a8a";
const UNPLAYED_COLOR = "#94a3b8";
const PLAYHEAD_COLOR = "#dc2626";
const RULER_TICK_MAJOR = "rgba(15, 23, 42, 0.38)";
const RULER_TICK_MINOR = "rgba(15, 23, 42, 0.14)";
const RULER_LABEL_COLOR = "rgba(15, 23, 42, 0.62)";
const CENTER_AXIS_COLOR = "rgba(15, 23, 42, 0.07)";
const RULER_LABEL_FONT =
  '10px ui-sans-serif, -apple-system, BlinkMacSystemFont, "Helvetica Neue", sans-serif';
// Time-ruler intervals (seconds) ordered shortest → longest. We pick the
// smallest entry whose pixel width is at least RULER_MIN_LABEL_PX so labels
// never overlap regardless of zoom.
const RULER_INTERVAL_CHOICES = [
  0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600,
] as const;
const RULER_MIN_LABEL_PX = 88;

// Audio enhancement defaults
const GAIN_MIN = 0;
const GAIN_MAX = 3;
const GAIN_STEP = 0.05;
const GAIN_DEFAULT = 1;
const RATE_MIN = 0.5;
const RATE_MAX = 2.5;
const RATE_STEP = 0.05;
const RATE_DEFAULT = 1;
const HPF_OFF = 0;
const LPF_OFF = 20000;
const HPF_OPTIONS = [
  { value: HPF_OFF, label: "Off" },
  { value: 60, label: "60 Hz" },
  { value: 100, label: "100 Hz" },
  { value: 150, label: "150 Hz" },
  { value: 220, label: "220 Hz" },
] as const;
const LPF_OPTIONS = [
  { value: LPF_OFF, label: "Off" },
  { value: 12000, label: "12 kHz" },
  { value: 8000, label: "8 kHz" },
  { value: 5000, label: "5 kHz" },
  { value: 3500, label: "3.5 kHz" },
] as const;

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
  const [thresholdDbfs, setThresholdDbfs] = useState(THRESHOLD_DBFS_DEFAULT);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(initialWaveform?.duration_seconds ?? 0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [enhanceOpen, setEnhanceOpen] = useState(false);
  const [gain, setGain] = useState(GAIN_DEFAULT);
  const [rate, setRate] = useState(RATE_DEFAULT);
  const [hpfHz, setHpfHz] = useState<number>(HPF_OFF);
  const [lpfHz, setLpfHz] = useState<number>(LPF_OFF);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const audioCtxRef = useRef<AudioContext | null>(null);
  const sourceNodeRef = useRef<MediaElementAudioSourceNode | null>(null);
  const gainNodeRef = useRef<GainNode | null>(null);
  const hpfNodeRef = useRef<BiquadFilterNode | null>(null);
  const lpfNodeRef = useRef<BiquadFilterNode | null>(null);
  const initialAudioPath = initialAudio?.audio_path ?? null;

  const pixelsPerSecond = zoom * PIXELS_PER_SECOND_AT_ZOOM_1;

  // Memoize the signed peak arrays once so every render path (detail canvas,
  // overview, skip-silence) reads from a stable view. Falls back to mirroring
  // the absolute peaks symmetrically when the backend payload lacks signed
  // data (older builds, unit-test fixtures).
  const detailPeaks = useMemo(() => {
    if (!waveform) {
      return { positive: [] as number[], negative: [] as number[] };
    }
    const length = waveform.peaks.length;
    const hasSigned =
      waveform.peaks_max?.length === length &&
      waveform.peaks_min?.length === length;
    if (hasSigned) {
      return {
        positive: waveform.peaks_max as number[],
        negative: waveform.peaks_min as number[],
      };
    }
    return {
      positive: waveform.peaks,
      negative: waveform.peaks.map((value) => -value),
    };
  }, [waveform?.peaks, waveform?.peaks_max, waveform?.peaks_min]);

  const overviewPeaks: SignedWaveformView = useMemo(
    () =>
      displaySignedWaveformPeaks(
        detailPeaks.positive,
        detailPeaks.negative,
        OVERVIEW_BAR_COUNT,
      ),
    [detailPeaks.positive, detailPeaks.negative],
  );

  const liveAudibleRanges = useMemo(() => {
    if (!waveform) {
      return [];
    }
    return audibleRangesFromPeaks(
      waveform.peaks,
      waveform.duration_seconds,
      thresholdDbfs,
    );
  }, [waveform?.peaks, waveform?.duration_seconds, thresholdDbfs]);

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
    void loadAudioWaveform(audio.audio_path, WAVEFORM_BUCKET_COUNT)
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

  useEffect(() => {
    setCurrentTime(0);
    setIsPlaying(false);
  }, [audioSource]);

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

  // Build Web Audio chain: source → HPF → LPF → Gain → destination.
  // The graph is rebuilt whenever the audio element is recreated (new source).
  useEffect(() => {
    if (typeof window === "undefined" || typeof AudioContext === "undefined") {
      return;
    }
    const element = audioRef.current;
    if (!element || !audioSource) {
      return;
    }

    let ctx = audioCtxRef.current;
    if (!ctx) {
      try {
        ctx = new AudioContext();
        audioCtxRef.current = ctx;
      } catch {
        return;
      }
    }

    let source: MediaElementAudioSourceNode;
    try {
      source = ctx.createMediaElementSource(element);
    } catch {
      // Re-creating the source for the same element throws; ignore.
      return;
    }
    const hpf = ctx.createBiquadFilter();
    hpf.type = "highpass";
    hpf.frequency.value = hpfHz <= HPF_OFF ? 1 : hpfHz;
    const lpf = ctx.createBiquadFilter();
    lpf.type = "lowpass";
    lpf.frequency.value = lpfHz >= LPF_OFF ? LPF_OFF : lpfHz;
    const gainNode = ctx.createGain();
    gainNode.gain.value = gain;

    source.connect(hpf);
    hpf.connect(lpf);
    lpf.connect(gainNode);
    gainNode.connect(ctx.destination);

    sourceNodeRef.current = source;
    hpfNodeRef.current = hpf;
    lpfNodeRef.current = lpf;
    gainNodeRef.current = gainNode;

    return () => {
      try {
        source.disconnect();
        hpf.disconnect();
        lpf.disconnect();
        gainNode.disconnect();
      } catch {
        // ignore
      }
      sourceNodeRef.current = null;
      hpfNodeRef.current = null;
      lpfNodeRef.current = null;
      gainNodeRef.current = null;
    };
  }, [audioSource]);

  useEffect(() => {
    const node = gainNodeRef.current;
    if (node) {
      node.gain.value = gain;
    }
  }, [gain]);

  useEffect(() => {
    const node = hpfNodeRef.current;
    if (node) {
      node.frequency.value = hpfHz <= HPF_OFF ? 1 : hpfHz;
    }
  }, [hpfHz]);

  useEffect(() => {
    const node = lpfNodeRef.current;
    if (node) {
      node.frequency.value = lpfHz >= LPF_OFF ? LPF_OFF : lpfHz;
    }
  }, [lpfHz]);

  useEffect(() => {
    const element = audioRef.current;
    if (element) {
      element.playbackRate = rate;
    }
  }, [rate, audioSource]);

  // High-frequency playhead + skip-silence loop while playing.
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
      if (skipSilence && liveAudibleRanges.length > 0) {
        const next = nextAudibleTime(element.currentTime, liveAudibleRanges);
        if (next !== null && next > element.currentTime) {
          element.currentTime = next;
        }
      }
      setCurrentTime(element.currentTime);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [isPlaying, skipSilence, liveAudibleRanges]);

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
    const ctx = audioCtxRef.current;
    if (ctx && ctx.state === "suspended") {
      void ctx.resume().catch(() => undefined);
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

  const handleNormalize = useCallback(() => {
    if (!waveform || waveform.peaks.length === 0) {
      return;
    }
    let maxPeak = 0;
    for (const peak of waveform.peaks) {
      if (Number.isFinite(peak) && peak > maxPeak) {
        maxPeak = peak;
      }
    }
    if (maxPeak <= 0.001) {
      return;
    }
    const normalized = Math.min(GAIN_MAX, 0.97 / maxPeak);
    setGain(Number(normalized.toFixed(2)));
  }, [waveform]);

  const handleResetEnhance = useCallback(() => {
    setGain(GAIN_DEFAULT);
    setRate(RATE_DEFAULT);
    setHpfHz(HPF_OFF);
    setLpfHz(LPF_OFF);
  }, []);

  const remainingTime = Math.max(0, duration - currentTime);

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
              <div className="audio-player">
                <div className="audio-player__timecode" aria-live="polite">
                  <span className="audio-player__time-current">
                    {formatTime(currentTime)}
                  </span>
                  <span className="audio-player__time-remaining">
                    -{formatTime(remainingTime)}
                  </span>
                </div>
                <WaveformOverview
                  positivePeaks={overviewPeaks.positive}
                  negativePeaks={overviewPeaks.negative}
                  duration={duration}
                  currentTime={currentTime}
                  audibleRanges={skipSilence ? liveAudibleRanges : []}
                  onSeek={seek}
                />
                <WaveformDetail
                  positivePeaks={detailPeaks.positive}
                  negativePeaks={detailPeaks.negative}
                  duration={duration}
                  currentTime={currentTime}
                  pixelsPerSecond={pixelsPerSecond}
                  thresholdDbfs={thresholdDbfs}
                  skipSilenceActive={skipSilence}
                  audibleRanges={liveAudibleRanges}
                  onSeek={seek}
                />
                <TransportControls
                  isPlaying={isPlaying}
                  onTogglePlay={togglePlay}
                  onSkip={skipBy}
                />
              </div>

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
                <div className="audio-threshold">
                  <div className="audio-threshold__header">
                    <span>Skip Silence Threshold</span>
                    <input
                      type="number"
                      className="audio-threshold__input"
                      value={thresholdDbfs}
                      min={THRESHOLD_DBFS_MIN}
                      max={THRESHOLD_DBFS_MAX}
                      step={THRESHOLD_DBFS_STEP}
                      aria-label="Skip silence threshold in dBFS"
                      onChange={(event) => {
                        const next = Number(event.currentTarget.value);
                        if (!Number.isFinite(next)) {
                          return;
                        }
                        setThresholdDbfs(
                          Math.max(
                            THRESHOLD_DBFS_MIN,
                            Math.min(THRESHOLD_DBFS_MAX, next),
                          ),
                        );
                      }}
                    />
                    <span className="audio-threshold__unit">dBFS</span>
                  </div>
                  <input
                    className="ui-slider audio-threshold__slider"
                    type="range"
                    value={thresholdDbfs}
                    min={THRESHOLD_DBFS_MIN}
                    max={THRESHOLD_DBFS_MAX}
                    step={THRESHOLD_DBFS_STEP}
                    aria-label="Skip silence threshold slider"
                    onChange={(event) =>
                      setThresholdDbfs(Number(event.currentTarget.value))
                    }
                  />
                </div>
                <div className="audio-workbench__switch">
                  <span>Skip Silence</span>
                  <Switch
                    label="Skip Silence"
                    checked={skipSilence}
                    onCheckedChange={setSkipSilence}
                  />
                </div>
              </div>

              <AudioEnhancePanel
                open={enhanceOpen}
                onToggle={() => setEnhanceOpen((prev) => !prev)}
                gain={gain}
                onGainChange={setGain}
                rate={rate}
                onRateChange={setRate}
                hpfHz={hpfHz}
                onHpfChange={setHpfHz}
                lpfHz={lpfHz}
                onLpfChange={setLpfHz}
                onNormalize={handleNormalize}
                onReset={handleResetEnhance}
              />
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

const OVERVIEW_HEIGHT = 48;

function WaveformOverview({
  positivePeaks,
  negativePeaks,
  duration,
  currentTime,
  audibleRanges,
  onSeek,
}: {
  positivePeaks: number[];
  negativePeaks: number[];
  duration: number;
  currentTime: number;
  audibleRanges: { start: number; end: number }[];
  onSeek: (time: number) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [width, setWidth] = useState(0);
  const progressRatio = duration > 0 ? Math.max(0, Math.min(1, currentTime / duration)) : 0;

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
    if (!canvas || width <= 0) {
      return;
    }
    const dpr =
      typeof window !== "undefined" ? Math.max(1, window.devicePixelRatio || 1) : 1;
    canvas.width = Math.max(1, Math.round(width * dpr));
    canvas.height = Math.max(1, Math.round(OVERVIEW_HEIGHT * dpr));
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, OVERVIEW_HEIGHT);
    const centerY = OVERVIEW_HEIGHT / 2;
    const halfHeight = (OVERVIEW_HEIGHT * 0.92) / 2;
    const count = Math.min(positivePeaks.length, negativePeaks.length);
    if (count === 0) {
      return;
    }
    const bucketWidth = width / count;
    const barWidth = Math.max(1, bucketWidth - 0.6);
    for (let index = 0; index < count; index += 1) {
      const positive = positivePeaks[index] ?? 0;
      const negative = negativePeaks[index] ?? 0;
      const upHeight = amplitudeToDisplay(positive) * halfHeight;
      const downHeight = amplitudeToDisplay(negative) * halfHeight;
      const x = Math.round(index * bucketWidth);
      const topY = Math.round(centerY - upHeight);
      const totalHeight = Math.max(1, Math.round(upHeight + downHeight));
      const playedBy = (index + 0.5) / count;
      ctx.fillStyle = playedBy <= progressRatio ? PLAYED_COLOR : UNPLAYED_COLOR;
      ctx.globalAlpha = 0.85;
      ctx.fillRect(x, topY, Math.max(1, Math.round(barWidth)), totalHeight);
    }
    ctx.globalAlpha = 1;
  }, [width, positivePeaks, negativePeaks, progressRatio]);

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
      <canvas
        ref={canvasRef}
        style={{ width: "100%", height: OVERVIEW_HEIGHT, display: "block" }}
      />
      {duration > 0
        ? audibleRanges.map((range) => (
            <span
              key={`audible-${range.start}-${range.end}`}
              className="audio-waveform-overview__range"
              style={{
                left: `${(range.start / duration) * 100}%`,
                width: `${Math.max(0.5, ((range.end - range.start) / duration) * 100)}%`,
              }}
            />
          ))
        : null}
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
  positivePeaks,
  negativePeaks,
  duration,
  currentTime,
  pixelsPerSecond,
  thresholdDbfs,
  skipSilenceActive,
  audibleRanges,
  onSeek,
}: {
  positivePeaks: number[];
  negativePeaks: number[];
  duration: number;
  currentTime: number;
  pixelsPerSecond: number;
  thresholdDbfs: number;
  skipSilenceActive: boolean;
  audibleRanges: { start: number; end: number }[];
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
    if (
      !canvas ||
      width <= 0 ||
      positivePeaks.length === 0 ||
      negativePeaks.length === 0 ||
      duration <= 0
    ) {
      return;
    }

    const dpr =
      typeof window !== "undefined" ? Math.max(1, window.devicePixelRatio || 1) : 1;
    const cssHeight = WAVEFORM_HEIGHT;
    canvas.width = Math.max(1, Math.round(width * dpr));
    canvas.height = Math.max(1, Math.round(cssHeight * dpr));
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, cssHeight);

    const rulerHeight = WAVEFORM_RULER_HEIGHT;
    const waveTop = rulerHeight;
    const waveBottom = cssHeight;
    const waveHeight = waveBottom - waveTop;
    const centerY = waveTop + waveHeight / 2;
    const halfHeight = (waveHeight * WAVEFORM_BAR_FILL_RATIO) / 2;
    const centerXf = width / 2;
    const centerX = Math.round(centerXf);

    // Bar geometry. 1px-wide bars with 1px gap (stride 2) keeps the canvas
    // pixel-aligned without resorting to subpixel blurring.
    const barWidth = 1;
    const barGap = 1;
    const barStride = barWidth + barGap;
    const peakLength = Math.min(positivePeaks.length, negativePeaks.length);
    const secondsPerPeak = duration / peakLength;
    const peaksPerBar = Math.max(
      1,
      Math.round(barStride / pixelsPerSecond / secondsPerPeak),
    );

    // 1. Silence shading (under the bars so it doesn't overpower them).
    if (skipSilenceActive && audibleRanges.length > 0) {
      ctx.fillStyle = "rgba(249, 115, 22, 0.08)";
      let cursor = 0;
      for (const range of audibleRanges) {
        if (range.start > cursor) {
          const x0 = Math.round(centerXf + (cursor - currentTime) * pixelsPerSecond);
          const x1 = Math.round(centerXf + (range.start - currentTime) * pixelsPerSecond);
          if (x1 > x0) {
            ctx.fillRect(x0, waveTop, x1 - x0, waveHeight);
          }
        }
        cursor = Math.max(cursor, range.end);
      }
      if (cursor < duration) {
        const x0 = Math.round(centerXf + (cursor - currentTime) * pixelsPerSecond);
        const x1 = Math.round(centerXf + (duration - currentTime) * pixelsPerSecond);
        if (x1 > x0) {
          ctx.fillRect(x0, waveTop, x1 - x0, waveHeight);
        }
      }
    }

    // 2. Subtle center axis line — anchors the eye, prevents tall asymmetric
    //    peaks from looking unbalanced.
    ctx.fillStyle = CENTER_AXIS_COLOR;
    ctx.fillRect(0, Math.round(centerY), width, 1);

    // 3. Time ruler ticks and labels.
    drawTimeRuler(ctx, {
      width,
      pixelsPerSecond,
      currentTime,
      duration,
      centerXf,
      rulerHeight,
    });

    // 4. The waveform itself. Bars step in barStride increments and align to
    //    integer pixel boundaries so they stay crisp at every zoom level.
    const startOffset = ((centerX % barStride) + barStride) % barStride;
    for (let x = startOffset; x < width; x += barStride) {
      const t = currentTime + (x - centerXf) / pixelsPerSecond;
      if (t < 0 || t > duration) {
        continue;
      }
      const startIndex = Math.min(peakLength - 1, Math.floor(t / secondsPerPeak));
      const endIndex = Math.min(peakLength, startIndex + peaksPerBar);
      let posMax = 0;
      let negMin = 0;
      for (let i = startIndex; i < endIndex; i += 1) {
        const positive = positivePeaks[i];
        const negative = negativePeaks[i];
        if (positive > posMax) {
          posMax = positive;
        }
        if (negative < negMin) {
          negMin = negative;
        }
      }
      // Apply perceptual dB scaling so quiet content remains visible.
      const upDisplay = amplitudeToDisplay(posMax);
      const downDisplay = amplitudeToDisplay(negMin);
      const upHeight = upDisplay * halfHeight;
      const downHeight = downDisplay * halfHeight;
      const totalHeight = Math.max(1, Math.round(upHeight + downHeight));
      const topY = Math.round(centerY - upHeight);
      ctx.fillStyle = t <= currentTime ? PLAYED_COLOR : UNPLAYED_COLOR;
      ctx.fillRect(x, topY, barWidth, totalHeight);
    }

    // 5. Skip-silence threshold reference line. Drawn on top of the bars,
    //    pixel-aligned to avoid the blurry shadow look from the previous
    //    implementation.
    const thresholdDisplay = dbfsToDisplay(thresholdDbfs);
    if (thresholdDisplay > 0) {
      const offset = thresholdDisplay * halfHeight;
      const upperY = Math.round(centerY - offset) + 0.5;
      const lowerY = Math.round(centerY + offset) + 0.5;
      ctx.save();
      ctx.strokeStyle = THRESHOLD_COLOR;
      ctx.lineWidth = 1;
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.moveTo(0, upperY);
      ctx.lineTo(width, upperY);
      ctx.moveTo(0, lowerY);
      ctx.lineTo(width, lowerY);
      ctx.stroke();
      ctx.restore();
    }

    // 6. Playhead — Voice Memos style: thin vertical accent line with a
    //    rounded disc at the top edge of the wave area.
    ctx.fillStyle = PLAYHEAD_COLOR;
    ctx.fillRect(centerX - 1, waveTop, 2, waveHeight);
    ctx.beginPath();
    ctx.arc(centerX, waveTop, 4.5, 0, Math.PI * 2);
    ctx.fill();
  }, [
    width,
    positivePeaks,
    negativePeaks,
    duration,
    currentTime,
    pixelsPerSecond,
    thresholdDbfs,
    skipSilenceActive,
    audibleRanges,
  ]);

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
      <span
        className="audio-waveform-detail__threshold-label"
        title={`Silence threshold: ${thresholdDbfs} dBFS`}
        aria-hidden
      >
        {formatThresholdLabel(thresholdDbfs)} dBFS
      </span>
    </div>
  );
}

function TransportControls({
  isPlaying,
  onTogglePlay,
  onSkip,
}: {
  isPlaying: boolean;
  onTogglePlay: () => void;
  onSkip: (offsetSeconds: number) => void;
}) {
  return (
    <div className="audio-transport">
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
  const isBack = offset < 0;
  const label = isBack
    ? `Skip back ${absSeconds} seconds`
    : `Skip forward ${absSeconds} seconds`;
  const Icon = isBack ? RotateCcw : RotateCw;
  return (
    <button
      type="button"
      className={`audio-transport__skip audio-transport__skip--${isBack ? "back" : "fwd"}`}
      onClick={() => onSkip(offset)}
      aria-label={label}
    >
      <Icon className="audio-transport__skip-icon" aria-hidden />
      <span className="audio-transport__skip-amount">{absSeconds}</span>
    </button>
  );
}

function AudioEnhancePanel({
  open,
  onToggle,
  gain,
  onGainChange,
  rate,
  onRateChange,
  hpfHz,
  onHpfChange,
  lpfHz,
  onLpfChange,
  onNormalize,
  onReset,
}: {
  open: boolean;
  onToggle: () => void;
  gain: number;
  onGainChange: (value: number) => void;
  rate: number;
  onRateChange: (value: number) => void;
  hpfHz: number;
  onHpfChange: (value: number) => void;
  lpfHz: number;
  onLpfChange: (value: number) => void;
  onNormalize: () => void;
  onReset: () => void;
}) {
  const enhancementsActive =
    gain !== GAIN_DEFAULT ||
    rate !== RATE_DEFAULT ||
    hpfHz !== HPF_OFF ||
    lpfHz !== LPF_OFF;

  return (
    <section className={`audio-enhance${open ? " audio-enhance--open" : ""}`}>
      <button
        type="button"
        className="audio-enhance__header"
        aria-expanded={open}
        onClick={onToggle}
      >
        <span className="audio-enhance__title">
          <Wand2 aria-hidden />
          Audio Enhance
          {enhancementsActive ? (
            <span className="audio-enhance__badge">on</span>
          ) : null}
        </span>
        <ChevronDown
          className="audio-enhance__chevron"
          aria-hidden
          data-open={open ? "true" : "false"}
        />
      </button>
      {open ? (
        <div className="audio-enhance__body">
          <div className="audio-enhance__grid">
            <Slider
              label="Volume Boost"
              min={GAIN_MIN}
              max={GAIN_MAX}
              step={GAIN_STEP}
              value={Number(gain.toFixed(2))}
              suffix={`x (${gainToDb(gain)} dB)`}
              onValueChange={onGainChange}
            />
            <Slider
              label="Playback Rate"
              min={RATE_MIN}
              max={RATE_MAX}
              step={RATE_STEP}
              value={Number(rate.toFixed(2))}
              suffix="x"
              onValueChange={onRateChange}
            />
            <label className="ui-field">
              <span className="ui-field__label">
                High-pass (remove rumble)
                <strong>
                  {HPF_OPTIONS.find((option) => option.value === hpfHz)?.label ??
                    `${hpfHz} Hz`}
                </strong>
              </span>
              <select
                className="ui-select"
                value={hpfHz}
                onChange={(event) => onHpfChange(Number(event.currentTarget.value))}
              >
                {HPF_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
            <label className="ui-field">
              <span className="ui-field__label">
                Low-pass (remove hiss)
                <strong>
                  {LPF_OPTIONS.find((option) => option.value === lpfHz)?.label ??
                    `${lpfHz} Hz`}
                </strong>
              </span>
              <select
                className="ui-select"
                value={lpfHz}
                onChange={(event) => onLpfChange(Number(event.currentTarget.value))}
              >
                {LPF_OPTIONS.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="audio-enhance__actions">
            <Button type="button" variant="secondary" size="sm" onClick={onNormalize}>
              Normalize
            </Button>
            <Button
              type="button"
              variant="ghost"
              size="sm"
              onClick={onReset}
              disabled={!enhancementsActive}
            >
              Reset
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  );
}

function pickRulerInterval(pixelsPerSecond: number): number {
  for (const candidate of RULER_INTERVAL_CHOICES) {
    if (candidate * pixelsPerSecond >= RULER_MIN_LABEL_PX) {
      return candidate;
    }
  }
  return RULER_INTERVAL_CHOICES[RULER_INTERVAL_CHOICES.length - 1];
}

function drawTimeRuler(
  ctx: CanvasRenderingContext2D,
  options: {
    width: number;
    pixelsPerSecond: number;
    currentTime: number;
    duration: number;
    centerXf: number;
    rulerHeight: number;
  },
) {
  const { width, pixelsPerSecond, currentTime, duration, centerXf, rulerHeight } =
    options;
  if (pixelsPerSecond <= 0 || rulerHeight <= 0 || duration <= 0) {
    return;
  }

  const major = pickRulerInterval(pixelsPerSecond);
  // Sub-tick every 1/5th of the major interval (or 1/4 when 1/5 would be
  // sub-second on the long-form choices) — keeps the rhythm readable.
  const minor = major >= 60 ? major / 4 : major / 5;

  const visibleSecondsHalf = width / pixelsPerSecond / 2;
  const start = Math.max(0, currentTime - visibleSecondsHalf - minor);
  const end = Math.min(duration, currentTime + visibleSecondsHalf + minor);
  const firstMinorIndex = Math.ceil(start / minor);
  const lastMinorIndex = Math.floor(end / minor);

  ctx.save();
  ctx.font = RULER_LABEL_FONT;
  ctx.textBaseline = "alphabetic";

  for (let index = firstMinorIndex; index <= lastMinorIndex; index += 1) {
    const time = index * minor;
    if (time < 0 || time > duration) {
      continue;
    }
    const x = Math.round(centerXf + (time - currentTime) * pixelsPerSecond);
    if (x < -2 || x > width + 2) {
      continue;
    }
    const isMajor =
      Math.abs(time / major - Math.round(time / major)) < 0.001;
    if (isMajor) {
      ctx.fillStyle = RULER_TICK_MAJOR;
      ctx.fillRect(x, 0, 1, Math.round(rulerHeight * 0.55));
      ctx.fillStyle = RULER_LABEL_COLOR;
      const label = formatRulerTime(time, major);
      const metrics = ctx.measureText(label);
      const labelX = Math.min(
        width - metrics.width - 2,
        Math.max(2, x + 4),
      );
      ctx.fillText(label, labelX, rulerHeight - 5);
    } else {
      ctx.fillStyle = RULER_TICK_MINOR;
      ctx.fillRect(x, 0, 1, Math.round(rulerHeight * 0.32));
    }
  }
  ctx.restore();
}

function formatRulerTime(seconds: number, interval: number): string {
  const safe = Math.max(0, seconds);
  const showFractional = interval < 1;
  if (showFractional) {
    const minutes = Math.floor(safe / 60);
    const remainder = safe - minutes * 60;
    const formatted = remainder.toFixed(1);
    return `${minutes}:${remainder < 10 ? "0" : ""}${formatted}`;
  }
  const totalSeconds = Math.round(safe);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const secs = totalSeconds % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

function gainToDb(value: number): string {
  if (value <= 0) {
    return "-∞";
  }
  const db = 20 * Math.log10(value);
  if (!Number.isFinite(db)) {
    return "-∞";
  }
  const rounded = Math.round(db * 10) / 10;
  return rounded > 0 ? `+${rounded}` : `${rounded}`;
}

function formatThresholdLabel(value: number): string {
  if (Number.isInteger(value)) {
    return String(value);
  }
  return value.toFixed(1);
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
