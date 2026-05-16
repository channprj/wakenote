import type { AudioRange } from "./types";

const DEFAULT_MIN_JUMP_SECONDS = 0.2;

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
