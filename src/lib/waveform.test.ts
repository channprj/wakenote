import { describe, expect, it } from "vitest";
import { displaySignedWaveformPeaks, visibleWaveformPeaks } from "./waveform";

describe("waveform view helpers", () => {
  it("returns the full waveform when zoom is at minimum", () => {
    expect(visibleWaveformPeaks([0, 0.25, 0.5, 1], 1, 0.5)).toEqual([0, 0.25, 0.5, 1]);
  });

  it("returns a centered subset when zoomed in", () => {
    expect(visibleWaveformPeaks([0, 0.25, 0.5, 0.75, 1], 2, 0.5)).toEqual([0.25, 0.5, 0.75]);
  });

  it("clamps the zoom window to the waveform edges", () => {
    expect(visibleWaveformPeaks([0, 0.25, 0.5, 0.75, 1], 3, 0)).toEqual([0, 0.25]);
  });
});

describe("displaySignedWaveformPeaks", () => {
  it("returns empty arrays when sources are empty", () => {
    expect(displaySignedWaveformPeaks([], [], 100)).toEqual({ positive: [], negative: [] });
  });

  it("returns the sources untouched when shorter than the target bar count", () => {
    const result = displaySignedWaveformPeaks([0.2, 0.4], [-0.1, -0.3], 8);
    expect(result.positive).toEqual([0.2, 0.4]);
    expect(result.negative).toEqual([-0.1, -0.3]);
  });

  it("buckets sources into the target bar count, taking max-pos and min-neg per bucket", () => {
    const positive = [0.1, 0.3, 0.2, 0.5, 0.4, 0.6];
    const negative = [-0.2, -0.05, -0.4, -0.1, -0.3, -0.5];
    const result = displaySignedWaveformPeaks(positive, negative, 3);
    expect(result.positive).toEqual([0.3, 0.5, 0.6]);
    expect(result.negative).toEqual([-0.2, -0.4, -0.5]);
  });

  it("uses the shorter array length when the inputs are uneven", () => {
    const result = displaySignedWaveformPeaks([0.5, 0.5, 0.5], [-0.4], 2);
    expect(result.positive).toEqual([0.5]);
    expect(result.negative).toEqual([-0.4]);
  });

  it("clamps non-positive bar counts to an empty view", () => {
    expect(displaySignedWaveformPeaks([0.5], [-0.5], 0)).toEqual({
      positive: [],
      negative: [],
    });
  });
});
