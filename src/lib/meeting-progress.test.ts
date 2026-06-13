import { describe, expect, it } from "vitest";
import {
  canResumeMeeting,
  formatClock,
  formatEta,
  isMeetingActive,
  meetingStatusLabel,
  meetingStatusTone,
  progressPercent,
} from "./meeting-progress";

describe("progressPercent", () => {
  it("returns an integer 0..100 by audio time", () => {
    expect(progressPercent(0, 1000)).toBe(0);
    expect(progressPercent(500, 1000)).toBe(50);
    expect(progressPercent(1000, 1000)).toBe(100);
  });

  it("clamps and guards a zero duration", () => {
    expect(progressPercent(100, 0)).toBe(0);
    expect(progressPercent(2000, 1000)).toBe(100);
  });
});

describe("formatClock", () => {
  it("uses m:ss under an hour and h:mm:ss above", () => {
    expect(formatClock(0)).toBe("0:00");
    expect(formatClock(65_000)).toBe("1:05");
    expect(formatClock(3_725_000)).toBe("1:02:05");
  });

  it("clamps invalid input", () => {
    expect(formatClock(-100)).toBe("0:00");
    expect(formatClock(Number.NaN)).toBe("0:00");
  });
});

describe("formatEta", () => {
  it("renders an em dash when unknown", () => {
    expect(formatEta(0)).toBe("—");
    expect(formatEta(-1)).toBe("—");
  });

  it("renders coarse human durations", () => {
    expect(formatEta(45_000)).toBe("약 45초");
    expect(formatEta(680_000)).toBe("약 11분 20초");
    expect(formatEta(3_660_000)).toBe("약 1시간 1분");
  });
});

describe("status presentation", () => {
  it("maps labels and tones", () => {
    expect(meetingStatusLabel("processing")).toBe("전사 중");
    expect(meetingStatusTone("completed")).toBe("success");
    expect(meetingStatusTone("failed")).toBe("danger");
  });

  it("classifies active and resumable states", () => {
    expect(isMeetingActive("processing")).toBe(true);
    expect(isMeetingActive("completed")).toBe(false);
    expect(canResumeMeeting("failed")).toBe(true);
    expect(canResumeMeeting("canceled")).toBe(true);
    expect(canResumeMeeting("completed")).toBe(false);
  });
});
