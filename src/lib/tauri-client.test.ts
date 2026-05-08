import { describe, expect, it } from "vitest";
import { mockSnapshot } from "./app-state";
import {
  chooseSaveRoot,
  cancelModelDownload,
  downloadModel,
  enqueueAudioFiles,
  processNextTranscription,
  saveSettingsPatch,
  skipJob,
  startLiveCapture,
  stopLiveCapture,
} from "./tauri-client";

describe("tauri live capture client", () => {
  it("returns browser-safe snapshots for live capture actions outside Tauri", async () => {
    const expected = mockSnapshot();

    const started = await startLiveCapture();
    expect(started.status).toMatchObject({
      live_input_active: true,
      tray_state: "listening",
    });

    const transcriptionOnly = await saveSettingsPatch({ recording_enabled: false });
    expect(transcriptionOnly.status).toMatchObject({
      live_input_active: false,
      tray_state: "idle",
    });

    await saveSettingsPatch({ recording_enabled: true });
    await expect(stopLiveCapture()).resolves.toEqual(expected);
    await expect(processNextTranscription()).resolves.toEqual(expected);
    await expect(chooseSaveRoot()).resolves.toEqual(expected);
  });

  it("simulates model download and cancel state outside Tauri", async () => {
    const downloading = await downloadModel("whisper-tiny");
    expect(downloading.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const cancelled = await cancelModelDownload("whisper-tiny");
    expect(cancelled.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "error",
      download_progress: 0,
      download_error: "cancelled by user",
    });
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

  it("processes the next queued browser fallback transcription", async () => {
    const before = await enqueueAudioFiles(["/tmp/imported/fallback.wav"]);
    const pendingBefore = before.queue.pending_count;
    const firstPendingId = before.queue.jobs.find((job) => job.status === "pending")?.id;

    const processed = await processNextTranscription();

    expect(processed.queue.pending_count).toBe(pendingBefore - 1);
    expect(processed.queue.running_count).toBe(0);
    expect(processed.queue.jobs.find((job) => job.id === firstPendingId)).toMatchObject({
      status: "completed",
      error: null,
    });
  });

  it("skips pending browser fallback queue jobs", async () => {
    const before = await enqueueAudioFiles(["/tmp/imported/skip-me.wav"]);
    const pendingBefore = before.queue.pending_count;
    const pendingId = before.queue.jobs.find((job) => job.status === "pending")?.id;
    expect(pendingId).toBeTypeOf("number");

    const skipped = await skipJob(pendingId ?? -1);

    expect(skipped.queue.pending_count).toBe(pendingBefore - 1);
    expect(skipped.queue.jobs.find((job) => job.id === pendingId)).toMatchObject({
      status: "skipped",
    });
  });
});
