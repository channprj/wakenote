import type { AudioRange } from "./types";

const DEFAULT_MIN_JUMP_SECONDS = 0.2;
const MIN_AUDIBLE_RANGE_SECONDS = 0.12;
const MERGE_SILENCE_GAP_SECONDS = 1.0;

/**
 * Finds the next audible time the playhead should advance to.
 *
 * Returns the start of the next audible range when `currentTime` falls inside a
 * silence gap. Returns `null` when the playhead is already inside an audible
 * range or when there is no future range to jump to.
 */
export function nextAudibleTime(
  currentTime: number,
  audibleRanges: AudioRange[],
  minJumpSeconds = DEFAULT_MIN_JUMP_SECONDS,
): number | null {
  if (!Number.isFinite(currentTime) || audibleRanges.length === 0) {
    return null;
  }

  for (const range of audibleRanges) {
    if (currentTime >= range.start && currentTime <= range.end) {
      return null;
    }
  }

  let bestStart: number | null = null;
  for (const range of audibleRanges) {
    if (range.start <= currentTime) {
      continue;
    }
    if (bestStart === null || range.start < bestStart) {
      bestStart = range.start;
    }
  }

  if (bestStart === null) {
    return null;
  }

  if (bestStart - currentTime < minJumpSeconds) {
    return null;
  }

  return bestStart;
}

/**
 * Returns the audible range covering a given time, or null when in silence.
 */
export function audibleRangeAt(
  currentTime: number,
  audibleRanges: AudioRange[],
): AudioRange | null {
  for (const range of audibleRanges) {
    if (currentTime >= range.start && currentTime <= range.end) {
      return range;
    }
  }
  return null;
}

/**
 * Converts a dBFS value to a 0..1 amplitude. Used to translate the user-facing
 * silence threshold into a peak comparison that mirrors the Rust backend.
 */
export function dbfsToAmplitude(dbfs: number): number {
  return 10 ** (dbfs / 20);
}

/** Default visual noise floor for waveform rendering, in dBFS. */
export const WAVEFORM_DISPLAY_FLOOR_DBFS = -60;

/**
 * Maps a dBFS value to a 0..1 display ratio using a linear-in-dB scale.
 * Anything at or below `floorDbfs` collapses to 0; 0 dBFS maps to 1. This is
 * how Voice Memos and most DAW meters keep quiet content readable instead of
 * disappearing under linear scaling.
 */
export function dbfsToDisplay(
  dbfs: number,
  floorDbfs = WAVEFORM_DISPLAY_FLOOR_DBFS,
): number {
  if (!Number.isFinite(dbfs) || floorDbfs >= 0) {
    return 0;
  }
  if (dbfs <= floorDbfs) {
    return 0;
  }
  if (dbfs >= 0) {
    return 1;
  }
  return (dbfs - floorDbfs) / -floorDbfs;
}

/**
 * Maps a linear amplitude in [-1, 1] to a display ratio in [0, 1] via the
 * perceptual dBFS curve. Negative inputs are folded through abs(); the
 * direction is expected to be applied by the renderer.
 */
export function amplitudeToDisplay(
  amplitude: number,
  floorDbfs = WAVEFORM_DISPLAY_FLOOR_DBFS,
): number {
  if (!Number.isFinite(amplitude) || amplitude === 0) {
    return 0;
  }
  const magnitude = Math.min(1, Math.abs(amplitude));
  if (magnitude <= 0) {
    return 0;
  }
  const dbfs = 20 * Math.log10(magnitude);
  return dbfsToDisplay(dbfs, floorDbfs);
}

/**
 * Recomputes audible ranges from the normalized peak amplitudes using a custom
 * threshold. Mirrors `next_audible_ranges_from_peaks` in the Rust backend so
 * the UI can react to threshold changes without a round trip.
 */
export function audibleRangesFromPeaks(
  peaks: number[],
  durationSeconds: number,
  thresholdDbfs: number,
): AudioRange[] {
  if (peaks.length === 0 || durationSeconds <= 0) {
    return [];
  }

  const threshold = dbfsToAmplitude(thresholdDbfs);
  const secondsPerBucket = durationSeconds / peaks.length;
  const ranges: AudioRange[] = [];
  let currentStart: number | null = null;

  for (let index = 0; index < peaks.length; index += 1) {
    const peak = peaks[index];
    const audible = Number.isFinite(peak) && peak >= threshold;
    if (audible) {
      if (currentStart === null) {
        currentStart = index * secondsPerBucket;
      }
      continue;
    }

    if (currentStart !== null) {
      pushAudibleRange(ranges, currentStart, index * secondsPerBucket, durationSeconds);
      currentStart = null;
    }
  }

  if (currentStart !== null) {
    pushAudibleRange(ranges, currentStart, durationSeconds, durationSeconds);
  }

  return ranges;
}

function pushAudibleRange(
  ranges: AudioRange[],
  rawStart: number,
  rawEnd: number,
  duration: number,
) {
  if (rawEnd - rawStart < MIN_AUDIBLE_RANGE_SECONDS) {
    return;
  }
  const start = Math.min(Math.max(0, rawStart), duration);
  const end = Math.min(Math.max(start, rawEnd), duration);
  const previous = ranges[ranges.length - 1];
  if (previous && start - previous.end <= MERGE_SILENCE_GAP_SECONDS) {
    previous.end = end;
    return;
  }
  ranges.push({ start, end });
}
