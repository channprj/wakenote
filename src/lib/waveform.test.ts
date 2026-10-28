import { describe, expect, it } from "vitest";
import {
  appendWaveformLevels,
  decayWaveformLevels,
  displaySignedWaveformPeaks,
  thresholdAmplitude,
  visibleWaveformPeaks,
  waveformBarCount,
} from "./waveform";

describe("waveform view helpers", () => {
  it("selects the visible waveform window for zoom and center", () => {
    const cases: Array<[string, Parameters<typeof visibleWaveformPeaks>, number[]]> = [
      ["full waveform at minimum zoom", [[0, 0.25, 0.5, 1], 1, 0.5], [0, 0.25, 0.5, 1]],
      ["centered subset when zoomed in", [[0, 0.25, 0.5, 0.75, 1], 2, 0.5], [0.25, 0.5, 0.75]],
      ["zoom window clamped to the waveform edges", [[0, 0.25, 0.5, 0.75, 1], 3, 0], [0, 0.25]],
    ];

    for (const [label, args, expected] of cases) {
      expect(visibleWaveformPeaks(...args), label).toEqual(expected);
    }
  });
});

describe("displaySignedWaveformPeaks", () => {
  it("buckets signed waveform peaks into the display bar count", () => {
    const cases: Array<[string, number[], number[], number, { positive: number[]; negative: number[] }]> = [
      ["empty sources", [], [], 100, { positive: [], negative: [] }],
      [
        "sources shorter than the target bar count stay untouched",
        [0.2, 0.4],
        [-0.1, -0.3],
        8,
        { positive: [0.2, 0.4], negative: [-0.1, -0.3] },
      ],
      [
        "max-pos and min-neg per bucket",
        [0.1, 0.3, 0.2, 0.5, 0.4, 0.6],
        [-0.2, -0.05, -0.4, -0.1, -0.3, -0.5],
        3,
        { positive: [0.3, 0.5, 0.6], negative: [-0.2, -0.4, -0.5] },
      ],
      [
        "uneven inputs use the shorter array length",
        [0.5, 0.5, 0.5],
        [-0.4],
        2,
        { positive: [0.5], negative: [-0.4] },
      ],
      ["non-positive bar counts clamp to an empty view", [0.5], [-0.5], 0, { positive: [], negative: [] }],
    ];

    for (const [label, positive, negative, bars, expected] of cases) {
      expect(displaySignedWaveformPeaks(positive, negative, bars), label).toEqual(expected);
    }
  });
});

describe("live waveform history", () => {
  it("appends finite clamped RMS values and retains the newest history", () => {
    expect(appendWaveformLevels([0.1, 0.2], [-1, 0.5, 2, Number.NaN], 4)).toEqual([
      0.2,
      0,
      0.5,
      1,
    ]);
  });

  it("maps the Rust -60..0 dBFS normalization to a visible threshold", () => {
    expect(thresholdAmplitude(-90)).toBe(0);
    expect(thresholdAmplitude(-40)).toBeCloseTo(1 / 3);
    expect(thresholdAmplitude(0)).toBe(1);
  });

  it("chooses a responsive bar count on an eight-bar visual rhythm", () => {
    expect(waveformBarCount(320)).toBe(48);
    expect(waveformBarCount(720)).toBe(96);
    expect(waveformBarCount(1440)).toBe(96);
  });

  it("decays every level toward zero", () => {
    expect(decayWaveformLevels([1, 0.5, 0], 0.5)).toEqual([0.5, 0.25, 0]);
  });
});
