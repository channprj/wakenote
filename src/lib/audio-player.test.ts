import { describe, expect, it } from "vitest";
import {
  amplitudeToDisplay,
  audibleRangeAt,
  audibleRangesFromPeaks,
  dbfsToAmplitude,
  dbfsToDisplay,
  nextAudibleTime,
} from "./audio-player";
import type { AudioRange } from "./types";

const ranges: AudioRange[] = [
  { start: 3, end: 8 },
  { start: 15, end: 20 },
];

describe("skip-silence playback helpers", () => {
  it("finds the next audible time to skip silence", () => {
    const cases: Array<[string, number, number | null]> = [
      ["jumps from leading silence to the first audible range", 0, 3],
      ["does not jump while playback is inside an audible range", 5, null],
      ["jumps across silence between audible ranges", 10, 15],
      ["does not jump after the final audible range", 24, null],
      ["jumps from the moment playback leaves a range", 8.05, 15],
      ["never jumps backwards even if an earlier range is the closest match", 16, null],
      ["suppresses tiny jumps that fall under the minimum threshold", 2.95, null],
    ];

    for (const [label, time, expected] of cases) {
      expect(nextAudibleTime(time, ranges), label).toBe(expected);
    }
  });
});

describe("audibleRangeAt", () => {
  it("returns the audible range at a playback time", () => {
    const cases: Array<[number, AudioRange | null]> = [
      [5, { start: 3, end: 8 }],
      [10, null],
    ];

    for (const [time, expected] of cases) {
      expect(audibleRangeAt(time, ranges), `t=${time}`).toEqual(expected);
    }
  });
});

describe("dbfsToAmplitude", () => {
  it("converts dBFS to linear amplitude", () => {
    const cases: Array<[number, number]> = [
      [0, 1],
      [-20, 0.1],
      [-60, 0.001],
    ];

    for (const [dbfs, amplitude] of cases) {
      expect(dbfsToAmplitude(dbfs), `${dbfs} dBFS`).toBeCloseTo(amplitude, 5);
    }
  });
});

describe("dbfsToDisplay", () => {
  it("maps dBFS onto the 0..1 waveform display scale", () => {
    // `exact` rows pin the clamped endpoints with toBe; the rest scale linearly.
    const cases: Array<{ args: Parameters<typeof dbfsToDisplay>; expected: number; exact?: true }> = [
      // Full scale maps to 1.
      { args: [0], expected: 1, exact: true },
      // Anything at or below the floor collapses to 0.
      { args: [-60], expected: 0, exact: true },
      { args: [-80], expected: 0, exact: true },
      // Linear between the floor and 0 dBFS.
      { args: [-30], expected: 0.5 },
      { args: [-15], expected: 0.75 },
      // Custom floor.
      { args: [-40, -80], expected: 0.5 },
      // Non-finite input or non-negative floors collapse to 0.
      { args: [Number.NaN], expected: 0, exact: true },
      { args: [-20, 0], expected: 0, exact: true },
      // Values above 0 dBFS clamp to 1.
      { args: [3], expected: 1, exact: true },
    ];

    for (const { args, expected, exact } of cases) {
      const label = `dbfsToDisplay(${args.join(", ")})`;
      if (exact) {
        expect(dbfsToDisplay(...args), label).toBe(expected);
      } else {
        expect(dbfsToDisplay(...args), label).toBeCloseTo(expected, 5);
      }
    }
  });
});

describe("amplitudeToDisplay", () => {
  it("maps amplitude onto the dB display scale", () => {
    // Rows without `digits` are exact.
    const cases: Array<{ label: string; amplitude: number; expected: number; digits?: number }> = [
      { label: "silence", amplitude: 0, expected: 0 },
      { label: "full scale", amplitude: 1, expected: 1 },
      { label: "non-finite input", amplitude: Number.NaN, expected: 0 },
      { label: "quiet signal on the dB scale", amplitude: 0.1, expected: (20 * Math.log10(0.1) + 60) / 60, digits: 4 },
      { label: "negative amplitude folds through abs()", amplitude: -0.5, expected: amplitudeToDisplay(0.5), digits: 6 },
    ];

    for (const { label, amplitude, expected, digits } of cases) {
      if (digits === undefined) {
        expect(amplitudeToDisplay(amplitude), label).toBe(expected);
      } else {
        expect(amplitudeToDisplay(amplitude), label).toBeCloseTo(expected, digits);
      }
    }
    // Quiet signals stay readable via the dB scale.
    expect(amplitudeToDisplay(0.1)).toBeGreaterThan(0.6);
  });
});

describe("audibleRangesFromPeaks", () => {
  // 1-second buckets covering 8 seconds total.
  // Indices 0,1 = silent; 2..5 = audible peaks; 6,7 = silent.
  const peaks = [0.0, 0.0, 0.4, 0.6, 0.5, 0.4, 0.0, 0.0];

  it("derives audible ranges from peak buckets", () => {
    const cases: Array<[string, Parameters<typeof audibleRangesFromPeaks>, AudioRange[]]> = [
      ["empty peaks", [[], 10, -45], []],
      ["zero duration", [peaks, 0, -45], []],
      ["generous threshold", [peaks, 8, -20], [{ start: 2, end: 6 }]],
      ["threshold above every peak", [peaks, 8, 0], []],
    ];

    for (const [label, args, expected] of cases) {
      expect(audibleRangesFromPeaks(...args), label).toEqual(expected);
    }
  });

  it("expands the audible span as the threshold drops below the noise floor", () => {
    const expanded = audibleRangesFromPeaks(peaks, 8, -80);
    expect(expanded).toHaveLength(1);
    expect(expanded[0].start).toBeLessThanOrEqual(2);
    expect(expanded[0].end).toBeGreaterThanOrEqual(6);
  });
});
