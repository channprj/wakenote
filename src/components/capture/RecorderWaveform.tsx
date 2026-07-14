import type { WaveformPhase } from "@/hooks/use-waveform-history";
import {
  displayWaveformPeaks,
  thresholdAmplitude,
  waveformBarCount,
} from "@/lib/waveform";

export type RecorderWaveformState = WaveformPhase | "listening";

export function RecorderWaveform({
  levels,
  width,
  currentDbfs,
  peakDbfs,
  thresholdDbfs,
  state,
}: {
  levels: readonly number[];
  width: number;
  currentDbfs: number;
  peakDbfs: number;
  thresholdDbfs: number;
  state: RecorderWaveformState;
}) {
  const barCount = waveformBarCount(width);
  const visibleLevels = displayWaveformPeaks([...levels], barCount);
  const bars = [
    ...Array.from({ length: Math.max(0, barCount - visibleLevels.length) }, () => 0),
    ...visibleLevels,
  ];
  const safeWidth = Math.max(1, width);
  const step = safeWidth / barCount;
  const barWidth = Math.max(1, Math.min(4.5, step * 0.56));
  const thresholdOffset = thresholdAmplitude(thresholdDbfs) * 44;
  const currentLabel = `Current ${Math.round(currentDbfs)} dBFS`;
  const peakLabel = `Peak ${Math.round(peakDbfs)} dBFS`;
  const thresholdLabel = `Threshold ${Math.round(thresholdDbfs)} dBFS`;

  return (
    <figure
      data-slot="recorder-waveform"
      data-state={state}
      data-bar-count={barCount}
      className="recorder-waveform"
    >
      <div className="recorder-waveform__readout">
        <span aria-label={currentLabel}>
          Current <strong>{Math.round(currentDbfs)} dBFS</strong>
        </span>
        <span aria-label={peakLabel}>
          Peak <strong>{Math.round(peakDbfs)} dBFS</strong>
        </span>
        <span aria-label={thresholdLabel}>
          Threshold <strong>{Math.round(thresholdDbfs)} dBFS</strong>
        </span>
      </div>
      <svg
        role="img"
        aria-label={`${state} microphone waveform. ${currentLabel}. ${peakLabel}. ${thresholdLabel}.`}
        viewBox={`0 0 ${safeWidth} 100`}
        preserveAspectRatio="none"
      >
        <line
          className="recorder-waveform__baseline"
          x1="0"
          x2={safeWidth}
          y1="50"
          y2="50"
          aria-hidden="true"
        />
        <line
          className="recorder-waveform__threshold"
          x1="0"
          x2={safeWidth}
          y1={50 - thresholdOffset}
          y2={50 - thresholdOffset}
          aria-hidden="true"
        />
        <line
          className="recorder-waveform__threshold"
          x1="0"
          x2={safeWidth}
          y1={50 + thresholdOffset}
          y2={50 + thresholdOffset}
          aria-hidden="true"
        />
        {bars.map((level, index) => {
          const height = Math.max(2, level * 88);
          return (
            <rect
              key={index}
              className="recorder-waveform__bar"
              data-level={level}
              data-silent={level === 0}
              x={index * step + (step - barWidth) / 2}
              y={50 - height / 2}
              width={barWidth}
              height={height}
              rx={barWidth / 2}
              aria-hidden="true"
            />
          );
        })}
      </svg>
    </figure>
  );
}
