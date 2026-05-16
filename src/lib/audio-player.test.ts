import { describe, expect, it } from "vitest";
import {
  audibleRangeAt,
  audibleRangesFromPeaks,
  dbfsToAmplitude,
  nextAudibleTime,
} from "./audio-player";
import type { AudioRange } from "./types";

const ranges: AudioRange[] = [
  { start: 3, end: 8 },
  { start: 15, end: 20 },
];

describe("skip-silence playback helpers", () => {
  it("jumps from leading silence to the first audible range", () => {
    expect(nextAudibleTime(0, ranges)).toBe(3);
  });

  it("does not jump while playback is inside an audible range", () => {
    expect(nextAudibleTime(5, ranges)).toBeNull();
  });

  it("jumps across silence between audible ranges", () => {
    expect(nextAudibleTime(10, ranges)).toBe(15);
  });

  it("does not jump after the final audible range", () => {
    expect(nextAudibleTime(24, ranges)).toBeNull();
  });

  it("jumps from the moment playback leaves a range", () => {
    expect(nextAudibleTime(8.05, ranges)).toBe(15);
  });

  it("never jumps backwards even if an earlier range is the closest match", () => {
    expect(nextAudibleTime(16, ranges)).toBeNull();
  });

  it("suppresses tiny jumps that fall under the minimum threshold", () => {
    expect(nextAudibleTime(2.95, ranges)).toBeNull();
  });
});

describe("audibleRangeAt", () => {
  it("returns the active range when inside one", () => {
    expect(audibleRangeAt(5, ranges)).toEqual({ start: 3, end: 8 });
  });

  it("returns null when in silence", () => {
    expect(audibleRangeAt(10, ranges)).toBeNull();
  });
});

describe("dbfsToAmplitude", () => {
  it("converts 0 dBFS to full-scale amplitude", () => {
    expect(dbfsToAmplitude(0)).toBeCloseTo(1, 5);
  });

  it("converts -20 dBFS to 0.1 amplitude", () => {
    expect(dbfsToAmplitude(-20)).toBeCloseTo(0.1, 5);
  });

  it("converts -60 dBFS to 0.001 amplitude", () => {
    expect(dbfsToAmplitude(-60)).toBeCloseTo(0.001, 5);
  });
});

describe("audibleRangesFromPeaks", () => {
  // 1-second buckets covering 8 seconds total.
  // Indices 0,1 = silent; 2..5 = audible peaks; 6,7 = silent.
  const peaks = [0.0, 0.0, 0.4, 0.6, 0.5, 0.4, 0.0, 0.0];

  it("returns an empty array when peaks are empty", () => {
    expect(audibleRangesFromPeaks([], 10, -45)).toEqual([]);
  });

  it("returns an empty array when duration is zero", () => {
    expect(audibleRangesFromPeaks(peaks, 0, -45)).toEqual([]);
  });

  it("detects audible spans with a generous threshold", () => {
    expect(audibleRangesFromPeaks(peaks, 8, -20)).toEqual([{ start: 2, end: 6 }]);
  });

  it("expands the audible span as the threshold drops below the noise floor", () => {
    const expanded = audibleRangesFromPeaks(peaks, 8, -80);
    expect(expanded).toHaveLength(1);
    expect(expanded[0].start).toBeLessThanOrEqual(2);
    expect(expanded[0].end).toBeGreaterThanOrEqual(6);
  });

  it("returns no ranges when the threshold is above every peak", () => {
    expect(audibleRangesFromPeaks(peaks, 8, 0)).toEqual([]);
  });
});
