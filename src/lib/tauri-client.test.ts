import { describe, expect, it } from "vitest";
import { mockSnapshot } from "./app-state";
import {
  chooseSaveRoot,
  cancelModelDownload,
  downloadModel,
  enqueueAudioFiles,
  processNextTranscription,
  startLiveCapture,
  stopLiveCapture,
} from "./tauri-client";

describe("tauri live capture client", () => {
  it("returns browser-safe snapshots for live capture actions outside Tauri", async () => {
    const expected = mockSnapshot();

    await expect(startLiveCapture()).resolves.toEqual(expected);
    await expect(stopLiveCapture()).resolves.toEqual(expected);
    await expect(processNextTranscription()).resolves.toEqual(expected);
    await expect(downloadModel("whisper-tiny")).resolves.toEqual(expected);
    await expect(cancelModelDownload("whisper-tiny")).resolves.toEqual(expected);
    await expect(chooseSaveRoot()).resolves.toEqual(expected);
  });

  it("queues manually selected audio files outside Tauri", async () => {
    const snapshot = await enqueueAudioFiles([
      "/tmp/imported/meeting.wav",
      "/tmp/imported/call.m4a",
    ]);

    expect(snapshot.queue.pending_count).toBe(2);
    expect(snapshot.queue.jobs.map((job) => job.audio_path)).toEqual([
      "/tmp/imported/meeting.wav",
      "/tmp/imported/call.m4a",
    ]);
    expect(snapshot.queue.jobs.every((job) => job.model_id === snapshot.settings.selected_model)).toBe(
      true,
    );
  });
});
