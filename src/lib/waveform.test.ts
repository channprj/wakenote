import { describe, expect, it } from "vitest";
import { visibleWaveformPeaks } from "./waveform";

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
