import { describe, expect, it } from "vitest";
import { reduceWaveformHistory } from "./use-waveform-history";

describe("reduceWaveformHistory", () => {
  it("appends new RMS frames and enters recording phase", () => {
    expect(
      reduceWaveformHistory(
        { levels: [0.2], phase: "recording" },
        { type: "levels", levels: [0.4, 0.8] },
      ),
    ).toEqual({ levels: [0.2, 0.4, 0.8], phase: "recording" });
  });

  it("starts decay after a recording with visible energy stops", () => {
    expect(
      reduceWaveformHistory(
        { levels: [0.8], phase: "recording" },
        { type: "stop" },
      ).phase,
    ).toBe("decaying");
  });

  it("returns to a clean idle state after the decay reaches zero", () => {
    expect(
      reduceWaveformHistory(
        { levels: [0, 0], phase: "decaying" },
        { type: "decay" },
      ),
    ).toEqual({ levels: [], phase: "idle" });
  });

  it("does not schedule decay for an empty stopped history", () => {
    expect(
      reduceWaveformHistory(
        { levels: [], phase: "idle" },
        { type: "stop" },
      ),
    ).toEqual({ levels: [], phase: "idle" });
  });
});
