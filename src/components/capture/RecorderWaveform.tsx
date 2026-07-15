import { memo, useMemo } from "react";
import type { WaveformPhase } from "@/hooks/use-waveform-history";
import {
  displayWaveformPeaks,
  thresholdAmplitude,
  waveformBarCount,
} from "@/lib/waveform";

const WAVEFORM_VIEWBOX_WIDTH = 720;
const WAVEFORM_BAR_COUNT = waveformBarCount(WAVEFORM_VIEWBOX_WIDTH);
const WAVEFORM_STEP = WAVEFORM_VIEWBOX_WIDTH / WAVEFORM_BAR_COUNT;
const WAVEFORM_BAR_WIDTH = Math.max(
  1,
  Math.min(4.5, WAVEFORM_STEP * 0.56),
);

export type RecorderWaveformState = WaveformPhase | "listening";

export const RecorderWaveform = memo(function RecorderWaveform({
  levels,
  currentDbfs,
  peakDbfs,
  thresholdDbfs,
  state,
}: {
  levels: readonly number[];
  currentDbfs: number;
  peakDbfs: number;
  thresholdDbfs: number;
  state: RecorderWaveformState;
}) {
  const bars = useMemo(() => {
    const visibleLevels = displayWaveformPeaks(
      [...levels],
      WAVEFORM_BAR_COUNT,
    );
    return [
      ...Array.from(
        { length: WAVEFORM_BAR_COUNT - visibleLevels.length },
        () => 0,
      ),
      ...visibleLevels,
    ];
  }, [levels]);
  const thresholdOffset = thresholdAmplitude(thresholdDbfs) * 44;
  const currentLabel = `Current ${Math.round(currentDbfs)} dBFS`;
  const peakLabel = `Peak ${Math.round(peakDbfs)} dBFS`;
  const thresholdLabel = `Threshold ${Math.round(thresholdDbfs)} dBFS`;

  return (
    <figure
      data-slot="recorder-waveform"
      data-state={state}
      data-bar-count={WAVEFORM_BAR_COUNT}
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
        viewBox={`0 0 ${WAVEFORM_VIEWBOX_WIDTH} 100`}
        preserveAspectRatio="none"
      >
        <line
          className="recorder-waveform__baseline"
          x1="0"
          x2={WAVEFORM_VIEWBOX_WIDTH}
          y1="50"
          y2="50"
          aria-hidden="true"
        />
        <line
          className="recorder-waveform__threshold"
          x1="0"
          x2={WAVEFORM_VIEWBOX_WIDTH}
          y1={50 - thresholdOffset}
          y2={50 - thresholdOffset}
          aria-hidden="true"
        />
        <line
          className="recorder-waveform__threshold"
          x1="0"
          x2={WAVEFORM_VIEWBOX_WIDTH}
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
              x={
                index * WAVEFORM_STEP +
                (WAVEFORM_STEP - WAVEFORM_BAR_WIDTH) / 2
              }
              y={50 - height / 2}
              width={WAVEFORM_BAR_WIDTH}
              height={height}
              rx={WAVEFORM_BAR_WIDTH / 2}
              aria-hidden="true"
            />
          );
        })}
      </svg>
    </figure>
  );
});
