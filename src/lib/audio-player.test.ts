import { describe, expect, it } from "vitest";
import { nextAudibleTime } from "./audio-player";
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
});
