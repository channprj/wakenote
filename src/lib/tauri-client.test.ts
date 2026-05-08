import { describe, expect, it } from "vitest";
import { defaultLevelSnapshot, mockSnapshot } from "./app-state";
import {
  chooseSaveRoot,
  cancelModelDownload,
  downloadModel,
  enqueueBacklog,
  enqueueAudioFiles,
  loadSnapshot,
  processNextTranscription,
  retryJob,
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

  it("simulates live browser fallback level snapshots while input is active", async () => {
    await startLiveCapture();

    const live = await loadSnapshot();

    expect(live.status.live_input_active).toBe(true);
    expect(live.status.level.current_dbfs).toBeGreaterThan(-120);
    expect(live.status.level.current_dbfs).toBeLessThanOrEqual(0);
    expect(live.status.level.noise_floor_dbfs).toBeLessThan(
      live.status.level.suggested_threshold_dbfs,
    );

    const stopped = await stopLiveCapture();

    expect(stopped.status.live_input_active).toBe(false);
    expect(stopped.status.level).toEqual(defaultLevelSnapshot());
  });

  it("reports recording state when simulated browser input crosses threshold", async () => {
    await saveSettingsPatch({ threshold_dbfs: -90 });

    const started = await startLiveCapture();

    expect(started.status.tray_state).toBe("recording");

    await stopLiveCapture();
    await saveSettingsPatch({ threshold_dbfs: -45 });
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

  it("does not duplicate manually imported browser fallback audio files", async () => {
    const audioPath = "/tmp/imported/duplicate-import.wav";

    await enqueueAudioFiles([audioPath]);
    const snapshot = await enqueueAudioFiles([audioPath]);

    expect(snapshot.queue.jobs.filter((job) => job.audio_path === audioPath)).toHaveLength(1);
  });

  it("does not duplicate browser fallback backlog audio files", async () => {
    const saveRoot = "/tmp/browser-backlog-dedupe";
    const audioPath = `${saveRoot}/20260506/230709.m4a`;

    await enqueueBacklog(saveRoot);
    const snapshot = await enqueueBacklog(saveRoot);

    expect(snapshot.queue.jobs.filter((job) => job.audio_path === audioPath)).toHaveLength(1);
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

  it("retries failed browser fallback queue jobs", async () => {
    const before = await enqueueAudioFiles(["/tmp/imported/retry-me.wav"]);
    const job = before.queue.jobs.find((candidate) => candidate.audio_path === "/tmp/imported/retry-me.wav");
    expect(job?.id).toBeTypeOf("number");
    if (job) {
      job.status = "failed";
      job.error = "mock failure";
    }

    const retried = await retryJob(job?.id ?? -1);

    expect(retried.queue.jobs.find((candidate) => candidate.id === job?.id)).toMatchObject({
      status: "pending",
      error: null,
    });
  });

  it("queues one simulated browser capture chunk after threshold activation", async () => {
    await saveSettingsPatch({ threshold_dbfs: -90, transcription_enabled: true });
    const before = await startLiveCapture();
    const pendingBefore = before.queue.pending_count;

    const captured = await loadSnapshot();

    expect(captured.status.tray_state).toBe("recording");
    expect(captured.queue.pending_count).toBe(pendingBefore + 1);

    const captureJob = captured.queue.jobs.at(-1);
    expect(captureJob).toMatchObject({
      model_id: captured.settings.selected_model,
      status: "pending",
      error: null,
    });
    expect(captureJob?.audio_path).toContain(captured.settings.save_root);
    expect(captureJob?.audio_path.endsWith(`.${captured.settings.audio_format}`)).toBe(true);

    const repeated = await loadSnapshot();

    expect(repeated.queue.pending_count).toBe(captured.queue.pending_count);
    expect(
      repeated.queue.jobs.filter((job) => job.audio_path === captureJob?.audio_path),
    ).toHaveLength(1);
  });

  it("does not duplicate browser capture sessions while input is already active", async () => {
    await stopLiveCapture();
    await saveSettingsPatch({ threshold_dbfs: -90, transcription_enabled: true });
    await startLiveCapture();
    const firstCapture = await loadSnapshot();

    await startLiveCapture();
    const repeatedStart = await loadSnapshot();

    expect(repeatedStart.status.live_input_active).toBe(true);
    expect(repeatedStart.queue.pending_count).toBe(firstCapture.queue.pending_count);
  });
});
