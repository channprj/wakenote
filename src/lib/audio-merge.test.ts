import { describe, expect, it } from "vitest";
import type { AudioMergeProgress, RecentTranscript } from "./types";
import {
  audioMergeDefaultFileName,
  createAudioMergeOperationId,
  eligibleAudioMergeEntries,
  nextAudioMergeProgress,
  type AudioMergeUiState,
} from "./audio-merge";

function transcript(
  transcriptPath: string,
  audioPath: string | null,
  recordedAt: string,
): RecentTranscript {
  return {
    transcript_path: transcriptPath,
    audio_path: audioPath,
    recorded_at: recordedAt,
    text: "Transcript",
  };
}

describe("audio merge UI contract", () => {
  it("requires at least two supported audio-backed transcript entries", () => {
    const entries = [
      transcript("/tmp/one.txt", "/tmp/one.M4A", "2026-08-04T05:00:01Z"),
      transcript("/tmp/no-audio.txt", null, "2026-08-04T05:00:02Z"),
    ];
    expect(eligibleAudioMergeEntries(entries)).toEqual([]);

    entries.push(
      transcript("/tmp/two.txt", "/tmp/two.wav", "2026-08-04T05:00:03Z"),
    );
    expect(
      eligibleAudioMergeEntries(entries).map((entry) => entry.audio_path),
    ).toEqual(["/tmp/one.M4A", "/tmp/two.wav"]);
  });

  it("uses earliest and latest local timestamps in the default file name", () => {
    const entries = [
      transcript(
        "/tmp/later.txt",
        "/tmp/later.m4a",
        "2026-08-04T14:05:06+09:00",
      ),
      transcript(
        "/tmp/earlier.txt",
        "/tmp/earlier.wav",
        "2026-08-04T09:01:02+09:00",
      ),
    ];

    expect(audioMergeDefaultFileName(entries)).toBe(
      "WakeNote-merged-20260804-090102-to-140506.m4a",
    );
  });

  it("creates an ASCII operation id with a deterministic fallback", () => {
    expect(createAudioMergeOperationId(1_722_764_800_000, null)).toBe(
      "merge-1722764800000-fallback",
    );
  });

  it("ignores other operations, never decreases progress, and exposes saved output", () => {
    const running: AudioMergeUiState = {
      status: "running",
      operation_id: "merge-current",
      stage: "converting",
      completed_inputs: 1,
      total_inputs: 2,
      percent: 55,
      current_input_label: "one.m4a",
    };
    const other: AudioMergeProgress = {
      operation_id: "merge-other",
      stage: "encoding",
      completed_inputs: 2,
      total_inputs: 2,
      percent: 90,
      current_input_label: null,
      destination_path: null,
    };
    expect(nextAudioMergeProgress(running, other)).toBe(running);

    const stale: AudioMergeProgress = {
      ...other,
      operation_id: "merge-current",
      stage: "combining",
      percent: 45,
    };
    const advanced = nextAudioMergeProgress(running, stale);
    expect(advanced.status).toBe("running");
    expect(advanced.status === "running" && advanced.percent).toBe(55);

    const saved = nextAudioMergeProgress(advanced, {
      ...stale,
      stage: "saved",
      percent: 100,
      destination_path: "/tmp/WakeNote merged.m4a",
    });
    expect(saved).toEqual({
      status: "saved",
      operation_id: "merge-current",
      destination_path: "/tmp/WakeNote merged.m4a",
      input_count: 2,
    });
  });
});
