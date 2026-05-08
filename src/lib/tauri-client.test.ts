import { describe, expect, it } from "vitest";
import { defaultLevelSnapshot, mockSnapshot } from "./app-state";
import {
  chooseSaveRoot,
  cancelModelDownload,
  cancelCurrentTranscription,
  deleteModel,
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
  verifyModel,
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
    await saveSettingsPatch({ threshold_dbfs: -90, transcription_enabled: false });

    const started = await startLiveCapture();

    expect(started.status.tray_state).toBe("recording");

    await stopLiveCapture();
    await saveSettingsPatch({ threshold_dbfs: -45, transcription_enabled: true });
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

  it("does not create cancelled errors for inactive browser fallback model downloads", async () => {
    await deleteModel("whisper-tiny");

    const cancelled = await cancelModelDownload("whisper-tiny");

    expect(cancelled.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "missing",
      download_progress: null,
      download_error: null,
    });
  });

  it("settles browser fallback model downloads during snapshot polling", async () => {
    await deleteModel("whisper-tiny");

    const downloading = await downloadModel("whisper-tiny");
    expect(downloading.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const settled = await loadSnapshot();

    expect(settled.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "ready",
      download_progress: 100,
      download_error: null,
    });
  });

  it("does not restart usable browser fallback model downloads", async () => {
    await deleteModel("whisper-tiny");
    await downloadModel("whisper-tiny");
    const ready = await loadSnapshot();
    expect(ready.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "ready",
      download_progress: 100,
      download_error: null,
    });

    const repeated = await downloadModel("whisper-tiny");

    expect(repeated.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "ready",
      download_progress: 100,
      download_error: null,
    });
  });

  it("does not delete active browser fallback model downloads", async () => {
    await deleteModel("whisper-tiny");
    const downloading = await downloadModel("whisper-tiny");
    expect(downloading.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const deleted = await deleteModel("whisper-tiny");

    expect(deleted.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });
  });

  it("does not verify active browser fallback model downloads", async () => {
    await deleteModel("whisper-tiny");
    const downloading = await downloadModel("whisper-tiny");
    expect(downloading.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const verified = await verifyModel("whisper-tiny");

    expect(verified.models.find((model) => model.id === "whisper-tiny")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });
  });

  it("settles browser fallback verification during snapshot polling", async () => {
    await deleteModel("whisper-medium");

    const verifying = await verifyModel("whisper-medium");
    expect(verifying.models.find((model) => model.id === "whisper-medium")).toMatchObject({
      status: "verifying",
      download_error: null,
    });

    const settled = await loadSnapshot();

    expect(settled.models.find((model) => model.id === "whisper-medium")).toMatchObject({
      status: "missing",
      download_progress: null,
      download_error: null,
    });
  });

  it("ignores browser fallback selected model patches for unusable models", async () => {
    await deleteModel("whisper-tiny");
    await downloadModel("whisper-tiny");
    await loadSnapshot();
    const selectedTiny = await saveSettingsPatch({ selected_model: "whisper-tiny" });
    expect(selectedTiny.settings.selected_model).toBe("whisper-tiny");

    await deleteModel("whisper-medium");
    const blocked = await saveSettingsPatch({ selected_model: "whisper-medium" });

    expect(blocked.settings.selected_model).toBe("whisper-tiny");
    expect(blocked.status.active_model).toBe("whisper-tiny");

    await downloadModel("whisper-medium");
    await loadSnapshot();
    const selectedMedium = await saveSettingsPatch({ selected_model: "whisper-medium" });

    expect(selectedMedium.settings.selected_model).toBe("whisper-medium");
    expect(selectedMedium.status.active_model).toBe("whisper-medium");
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

  it("ignores non-audio browser fallback imports", async () => {
    const before = await loadSnapshot();
    const snapshot = await enqueueAudioFiles([
      "/tmp/imported/not-audio.txt",
      "/tmp/imported/folder",
      "/tmp/imported/voice.WAV",
    ]);

    expect(snapshot.queue.pending_count).toBe(before.queue.pending_count + 1);
    expect(snapshot.queue.jobs.map((job) => job.audio_path)).toContain("/tmp/imported/voice.WAV");
    expect(snapshot.queue.jobs.map((job) => job.audio_path)).not.toContain(
      "/tmp/imported/not-audio.txt",
    );
    expect(snapshot.queue.jobs.map((job) => job.audio_path)).not.toContain(
      "/tmp/imported/folder",
    );
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

  it("cancels running browser fallback transcription jobs", async () => {
    const before = await enqueueAudioFiles(["/tmp/imported/cancel-running.wav"]);
    const job = before.queue.jobs.find(
      (candidate) => candidate.audio_path === "/tmp/imported/cancel-running.wav",
    );
    expect(job?.id).toBeTypeOf("number");
    if (job) {
      job.status = "running";
      before.queue.pending_count -= 1;
      before.queue.running_count += 1;
    }

    const cancelled = await cancelCurrentTranscription();

    expect(cancelled.queue.running_count).toBe(0);
    expect(cancelled.queue.jobs.find((candidate) => candidate.id === job?.id)).toMatchObject({
      status: "cancelled",
      error: "cancelled by user",
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

  it("clears stale browser fallback queue errors when jobs are skipped", async () => {
    const before = await enqueueAudioFiles(["/tmp/imported/skip-failed.wav"]);
    const job = before.queue.jobs.find(
      (candidate) => candidate.audio_path === "/tmp/imported/skip-failed.wav",
    );
    expect(job?.id).toBeTypeOf("number");
    if (job) {
      job.status = "failed";
      job.error = "mock failure";
    }

    const skipped = await skipJob(job?.id ?? -1);

    expect(skipped.queue.jobs.find((candidate) => candidate.id === job?.id)).toMatchObject({
      status: "skipped",
      error: null,
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

  it("finalizes a simulated browser capture when recording is disabled", async () => {
    await stopLiveCapture();
    await saveSettingsPatch({
      recording_enabled: true,
      threshold_dbfs: -90,
      transcription_enabled: true,
    });
    const active = await startLiveCapture();
    const pendingBefore = active.queue.pending_count;

    const disabled = await saveSettingsPatch({ recording_enabled: false });

    expect(disabled.status.live_input_active).toBe(false);
    expect(disabled.status.tray_state).toBe("idle");
    expect(disabled.queue.pending_count).toBe(pendingBefore + 1);
    expect(disabled.queue.jobs.at(-1)).toMatchObject({
      model_id: disabled.settings.selected_model,
      status: "pending",
      error: null,
    });
  });

  it("preserves a simulated browser capture when transcription is disabled mid-recording", async () => {
    await stopLiveCapture();
    await saveSettingsPatch({
      recording_enabled: true,
      threshold_dbfs: -90,
      transcription_enabled: true,
    });
    const active = await startLiveCapture();
    const pendingBefore = active.queue.pending_count;

    const disabled = await saveSettingsPatch({ transcription_enabled: false });

    expect(disabled.status.live_input_active).toBe(true);
    expect(disabled.status.mode).toBe("recording_only");
    expect(disabled.queue.pending_count).toBe(pendingBefore + 1);
    expect(disabled.queue.jobs.at(-1)).toMatchObject({
      model_id: active.settings.selected_model,
      status: "pending",
      error: null,
    });

    await stopLiveCapture();
  });

  it("finalizes a simulated browser capture when input is stopped", async () => {
    await saveSettingsPatch({
      recording_enabled: true,
      threshold_dbfs: -90,
      transcription_enabled: true,
    });
    const active = await startLiveCapture();
    const pendingBefore = active.queue.pending_count;

    const stopped = await stopLiveCapture();

    expect(stopped.status.live_input_active).toBe(false);
    expect(stopped.status.level).toEqual(defaultLevelSnapshot());
    expect(stopped.queue.pending_count).toBe(pendingBefore + 1);
    expect(stopped.queue.jobs.at(-1)).toMatchObject({
      model_id: stopped.settings.selected_model,
      status: "pending",
      error: null,
    });
  });
});
