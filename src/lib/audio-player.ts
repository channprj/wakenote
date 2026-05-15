import type { AudioRange } from "./types";

export function nextAudibleTime(
  currentTime: number,
  audibleRanges: AudioRange[],
  minJumpSeconds = 0.25,
): number | null {
  if (!Number.isFinite(currentTime) || audibleRanges.length === 0) {
    return null;
  }

  for (const range of audibleRanges) {
    if (currentTime >= range.start && currentTime <= range.end) {
      return null;
    }

    if (currentTime < range.start && range.start - currentTime >= minJumpSeconds) {
      return range.start;
    }
  }

  return null;
}
