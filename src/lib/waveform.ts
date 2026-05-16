export function visibleWaveformPeaks(
  peaks: number[],
  zoom: number,
  centerRatio: number,
): number[] {
  if (peaks.length === 0) {
    return [];
  }

  const safeZoom = Math.max(1, Math.min(16, zoom));
  if (safeZoom <= 1) {
    return peaks;
  }

  const windowSize = Math.max(1, Math.ceil(peaks.length / safeZoom));
  const centerIndex = Math.round(
    Math.max(0, Math.min(1, centerRatio)) * Math.max(0, peaks.length - 1),
  );
  const start = Math.max(0, Math.min(peaks.length - windowSize, centerIndex - Math.floor(windowSize / 2)));
  return peaks.slice(start, start + windowSize);
}

export function displayWaveformPeaks(peaks: number[], maxBars: number): number[] {
  if (peaks.length <= maxBars) {
    return peaks;
  }

  return Array.from({ length: maxBars }, (_, index) => {
    const start = Math.floor(index * peaks.length / maxBars);
    const end = Math.max(start + 1, Math.floor((index + 1) * peaks.length / maxBars));
    return peaks.slice(start, end).reduce((peak, value) => Math.max(peak, value), 0);
  });
}

export interface SignedWaveformView {
  positive: number[];
  negative: number[];
}

/**
 * Down-samples paired signed peaks into a target bar count. Each output bucket
 * holds the maximum positive peak and the minimum negative peak from the
 * source range, preserving the natural asymmetry of an audio signal. When the
 * arrays are shorter than the target count they are returned untouched.
 */
export function displaySignedWaveformPeaks(
  positive: number[],
  negative: number[],
  maxBars: number,
): SignedWaveformView {
  const length = Math.min(positive.length, negative.length);
  if (length === 0 || maxBars <= 0) {
    return { positive: [], negative: [] };
  }
  if (length <= maxBars) {
    return {
      positive: positive.slice(0, length),
      negative: negative.slice(0, length),
    };
  }

  const outPositive = new Array<number>(maxBars);
  const outNegative = new Array<number>(maxBars);
  for (let index = 0; index < maxBars; index += 1) {
    const start = Math.floor((index * length) / maxBars);
    const end = Math.max(start + 1, Math.floor(((index + 1) * length) / maxBars));
    let bucketMax = 0;
    let bucketMin = 0;
    for (let i = start; i < end; i += 1) {
      const pos = positive[i] ?? 0;
      const neg = negative[i] ?? 0;
      if (pos > bucketMax) bucketMax = pos;
      if (neg < bucketMin) bucketMin = neg;
    }
    outPositive[index] = bucketMax;
    outNegative[index] = bucketMin;
  }
  return { positive: outPositive, negative: outNegative };
}
