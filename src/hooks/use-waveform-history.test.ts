import { describe, expect, it } from "vitest";
import {
  createAnimationFrameBatcher,
  reduceWaveformHistory,
} from "./use-waveform-history";

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

describe("createAnimationFrameBatcher", () => {
  it("coalesces twenty level payloads into the latest value for one frame", () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    const committed: number[][] = [];
    let nextHandle = 0;

    const batcher = createAnimationFrameBatcher<number[]>(
      (levels) => committed.push(levels),
      (callback) => {
        nextHandle += 1;
        callbacks.set(nextHandle, callback);
        return nextHandle;
      },
      (handle) => callbacks.delete(handle),
    );

    for (let index = 0; index < 20; index += 1) {
      batcher.push([index / 20]);
    }

    expect(committed).toEqual([]);
    expect(callbacks.size).toBe(1);

    const pendingFrame = callbacks.get(1);
    callbacks.delete(1);
    pendingFrame?.(16);

    expect(committed).toEqual([[0.95]]);
    expect(callbacks.size).toBe(0);
  });

  it("cancels a pending frame without committing stale levels", () => {
    const callbacks = new Map<number, FrameRequestCallback>();
    const committed: number[][] = [];
    const batcher = createAnimationFrameBatcher<number[]>(
      (levels) => committed.push(levels),
      (callback) => {
        callbacks.set(1, callback);
        return 1;
      },
      (handle) => callbacks.delete(handle),
    );

    batcher.push([0.8]);
    const pendingFrame = callbacks.get(1);
    batcher.cancel();
    pendingFrame?.(16);

    expect(committed).toEqual([]);
    expect(callbacks.size).toBe(0);
  });
});
