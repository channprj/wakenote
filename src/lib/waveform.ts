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
