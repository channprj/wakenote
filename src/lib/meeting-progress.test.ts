import { describe, expect, it } from "vitest";
import {
  canStartMeetingTranscription,
  canResumeMeeting,
  formatClock,
  formatEta,
  isMeetingActive,
  meetingStatusLabel,
  meetingStatusTone,
  meetingPhaseLabel,
  progressPercent,
} from "./meeting-progress";

describe("progressPercent", () => {
  it("returns a clamped integer percent by audio time", () => {
    const cases: Array<[number, number, number]> = [
      [0, 1000, 0],
      [500, 1000, 50],
      [1000, 1000, 100],
      // Guards a zero duration and clamps overshoot.
      [100, 0, 0],
      [2000, 1000, 100],
    ];

    for (const [done, total, expected] of cases) {
      expect(progressPercent(done, total), `${done}/${total}`).toBe(expected);
    }
  });
});

describe("formatClock", () => {
  it("formats a clamped m:ss or h:mm:ss clock", () => {
    const cases: Array<[number, string]> = [
      // m:ss under an hour and h:mm:ss above.
      [0, "0:00"],
      [65_000, "1:05"],
      [3_725_000, "1:02:05"],
      // Invalid input clamps to zero.
      [-100, "0:00"],
      [Number.NaN, "0:00"],
    ];

    for (const [ms, expected] of cases) {
      expect(formatClock(ms), String(ms)).toBe(expected);
    }
  });
});

describe("formatEta", () => {
  it("formats a coarse ETA or an em dash when unknown", () => {
    const cases: Array<[number, string]> = [
      [0, "—"],
      [-1, "—"],
      [45_000, "about 45s"],
      [680_000, "about 11m 20s"],
      [3_660_000, "about 1h 1m"],
    ];

    for (const [ms, expected] of cases) {
      expect(formatEta(ms), String(ms)).toBe(expected);
    }
  });
});

describe("status presentation", () => {
  it("labels every long-running meeting phase", () => {
    expect(meetingPhaseLabel("preparing_audio")).toBe("Preparing audio");
    expect(meetingPhaseLabel("splitting_audio")).toBe("Splitting audio");
    expect(meetingPhaseLabel("transcribing")).toBe("Transcribing");
    expect(meetingPhaseLabel("finalizing")).toBe("Finalizing transcript");
  });

  it("maps labels and tones", () => {
    expect(meetingStatusLabel("recorded")).toBe("Recorded");
    expect(meetingStatusTone("recorded")).toBe("neutral");
    expect(meetingStatusLabel("pending")).toBe("Queued");
    expect(meetingStatusTone("pending")).toBe("warning");
    expect(meetingStatusLabel("processing")).toBe("Transcribing");
    expect(meetingStatusTone("completed")).toBe("success");
    expect(meetingStatusTone("failed")).toBe("danger");
  });

  it("classifies active and resumable states", () => {
    expect(isMeetingActive("recorded")).toBe(false);
    expect(isMeetingActive("pending")).toBe(true);
    expect(isMeetingActive("processing")).toBe(true);
    expect(isMeetingActive("completed")).toBe(false);
    expect(canResumeMeeting("pending")).toBe(true);
    expect(canResumeMeeting("recorded")).toBe(false);
    expect(canResumeMeeting("failed")).toBe(true);
    expect(canResumeMeeting("canceled")).toBe(true);
    expect(canResumeMeeting("completed")).toBe(false);
  });

  it("starts a new transcription only from terminal retryable states", () => {
    expect(canStartMeetingTranscription("recorded")).toBe(true);
    expect(canStartMeetingTranscription("failed")).toBe(true);
    expect(canStartMeetingTranscription("canceled")).toBe(true);
    expect(canStartMeetingTranscription("pending")).toBe(false);
    expect(canStartMeetingTranscription("processing")).toBe(false);
    expect(canStartMeetingTranscription("completed")).toBe(false);
  });
});
