import { describe, expect, it, vi } from "vitest";
import { defaultLevelSnapshot, mockSnapshot } from "./app-state";
import {
  chooseSaveRoot,
  revealSaveFolder,
  cancelCurrentOperation,
  cancelModelDownload,
  cancelCurrentTranscription,
  cancelLlmReport,
  deleteModel,
  deleteOpenRouterApiKey,
  deleteOpenAiApiKey,
  downloadModel,
  enqueueBacklog,
  enqueueAudioFiles,
  listLlmReportHistory,
  listLlmReportRuns,
  loadLlmReportHistoryDetail,
  loadSnapshot,
  loadTranscriptDays,
  loadTranscriptsForDay,
  processNextTranscription,
  regenerateTranscript,
  openTranscriptFolder,
  retryJob,
  retryLlmReport,
  saveOpenRouterApiKey,
  saveOpenAiApiKey,
  saveSettingsPatch,
  skipJob,
  startLiveCapture,
  startLlmReport,
  stopLiveCapture,
  startSourceCapture,
  stopSourceCapture,
  subscribeLlmReportRuns,
  loadRecognizedSources,
  loadSourceCaptureStatus,
  verifyModel,
} from "./tauri-client";

function browserCaptureNumber(audioPath: string | undefined) {
  const match = audioPath?.match(/browser-capture-(\d+)\./);
  return match ? Number(match[1]) : null;
}

describe("transcript day loaders (browser fallback)", () => {
  // Only verifies the empty-state contract: the browser snapshot's
  // recent_transcripts isn't seedable with controllable days in this harness
  // (browserSnapshot is module-private and the only writer, processNextTranscription,
  // always stamps the current time), so grouping/count/filter isn't exercised here.
  it("returns an array of days and an empty list for a day with no entries", async () => {
    const days = await loadTranscriptDays();
    expect(Array.isArray(days)).toBe(true);

    const none = await loadTranscriptsForDay("2099-01-01");
    expect(none).toEqual([]);
  });
});

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
    await expect(revealSaveFolder()).resolves.toEqual(expected);
  });

  it("marks browser fallback save root patches confirmed only when non-empty", async () => {
    const confirmed = await saveSettingsPatch({ save_root: "/tmp/confirmed-wakenote" });
    expect(confirmed.settings.save_root).toBe("/tmp/confirmed-wakenote");
    expect(confirmed.settings.save_root_confirmed).toBe(true);

    const blank = await saveSettingsPatch({ save_root: "   " });
    expect(blank.settings.save_root).toBe("   ");
    expect(blank.settings.save_root_confirmed).toBe(false);

    await saveSettingsPatch({ save_root: "/tmp/confirmed-wakenote" });
  });

  it("validates browser fallback dictation shortcut patches like the backend", async () => {
    const normalized = await saveSettingsPatch({
      dictation_shortcut: " Command + Alt + D ",
    });
    expect(normalized.settings.dictation_shortcut).toBe("alt+cmd+d");

    await expect(
      saveSettingsPatch({ dictation_shortcut: "   " }),
    ).rejects.toThrow("cannot be empty");
    const singleKey = await saveSettingsPatch({ dictation_shortcut: "space" });
    expect(singleKey.settings.dictation_shortcut).toBe("space");
    const physicalModifier = await saveSettingsPatch({
      dictation_shortcut: "LeftControl",
    });
    expect(physicalModifier.settings.dictation_shortcut).toBe("leftctrl");

    const unchanged = await loadSnapshot();
    expect(unchanged.settings.dictation_shortcut).toBe("leftctrl");

    const canonical = await saveSettingsPatch({ dictation_shortcut: "  Ctrl+Alt+D  " });
    expect(canonical.settings.dictation_shortcut).toBe("ctrl+alt+d");

    const enabled = await saveSettingsPatch({
      dictation_enabled: true,
      dictation_language: "en",
    });
    expect(enabled.settings.dictation_enabled).toBe(true);
    expect(enabled.settings.dictation_language).toBe("en");
    await saveSettingsPatch({ dictation_enabled: false, dictation_language: "auto" });
  });

  it("persists browser fallback theme mode patches", async () => {
    const light = await saveSettingsPatch({ theme_mode: "light" });
    expect(light.settings.theme_mode).toBe("light");

    const dark = await saveSettingsPatch({ theme_mode: "dark" });
    expect(dark.settings.theme_mode).toBe("dark");
  });

  it("tracks browser fallback OpenAI key state without retaining blank values", async () => {
    await deleteOpenAiApiKey();
    expect((await loadSnapshot()).openai_key_configured).toBe(false);

    await expect(saveOpenAiApiKey("   ")).rejects.toThrow(
      "OpenAI API key cannot be blank",
    );
    const saved = await saveOpenAiApiKey(" sk-openai-browser ");
    expect(saved.openai_key_configured).toBe(true);

    await deleteOpenAiApiKey();
    expect((await loadSnapshot()).openai_key_configured).toBe(false);
  });

  it("tracks browser fallback OpenRouter key state and completed durable runs", async () => {
    vi.useFakeTimers();
    await deleteOpenRouterApiKey();
    const missing = await loadSnapshot();
    expect(missing.openrouter_key_configured).toBe(false);

    await expect(
      startLlmReport({
        kind: "summary",
        transcripts: [
          {
            transcript_path: "/tmp/WakeNote/20260713/100000.txt",
            audio_path: null,
            recorded_at: "2026-07-13T10:00:00+09:00",
            text: "browser summary source",
            source: "microphone",
            source_label: null,
          },
        ],
      }),
    ).rejects.toThrow("OpenRouter API key is not configured");

    const saved = await saveOpenRouterApiKey(" sk-or-browser ");
    expect(saved.openrouter_key_configured).toBe(true);

    const started = await startLlmReport({
      kind: "summary",
      transcripts: [
        {
          transcript_path: "/tmp/WakeNote/20260713/100000.txt",
          audio_path: null,
          recorded_at: "2026-07-13T10:00:00+09:00",
          text: "browser summary source",
          source: "microphone",
          source_label: null,
        },
      ],
    });
    await vi.runAllTimersAsync();
    vi.useRealTimers();

    const completed = (await listLlmReportRuns()).find(
      (run) => run.run_id === started.run_id,
    );
    expect(completed).toMatchObject({
      status: "completed",
      model: "z-ai/glm-5.2",
      success_criteria_met: true,
    });
    expect(completed?.report_path).toContain("/reports/");
    expect(completed?.usage?.total_tokens).toBeGreaterThan(0);
    const history = await listLlmReportHistory();
    expect(history[0].report_id).toBe(completed?.report_id);
    const detail = await loadLlmReportHistoryDetail(
      completed?.report_id ?? "",
    );
    expect(detail.content).toContain("# Summary");
  });

  it("cancels browser report work without publishing history and retries it", async () => {
    vi.useFakeTimers();
    await saveOpenRouterApiKey("sk-or-browser");
    const historyBefore = await listLlmReportHistory();
    const statuses: string[] = [];
    const unsubscribe = await subscribeLlmReportRuns((run) => {
      statuses.push(run.status);
    });
    const request = {
      kind: "summary" as const,
      transcripts: [{
        transcript_path: "/tmp/source.txt",
        audio_path: null,
        recorded_at: "2026-07-18T09:00:00+09:00",
        text: "source",
        source: "microphone" as const,
        source_label: null,
      }],
    };

    const first = await startLlmReport(request);
    await expect(startLlmReport(request)).rejects.toThrow(
      first.run_id,
    );
    await vi.advanceTimersByTimeAsync(20);
    const stopping = await cancelLlmReport(first.run_id);
    expect(stopping.status).toBe("stopping");
    await vi.runAllTimersAsync();

    const cancelled = (await listLlmReportRuns()).find(
      (run) => run.run_id === first.run_id,
    );
    expect(cancelled?.status).toBe("cancelled");
    expect(await listLlmReportHistory()).toHaveLength(
      historyBefore.length,
    );

    const retry = await retryLlmReport(first.run_id);
    expect(retry.parent_run_id).toBe(first.run_id);
    expect(retry.run_id).not.toBe(first.run_id);
    await cancelLlmReport(retry.run_id);
    await vi.runAllTimersAsync();
    unsubscribe();
    vi.useRealTimers();

    expect(statuses).toContain("queued");
    expect(statuses).toContain("running");
    expect(statuses).toContain("stopping");
    expect(statuses).toContain("cancelled");
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

  it("does not process browser fallback queue work while the selected model is unavailable", async () => {
    const before = await enqueueAudioFiles(["/tmp/imported/missing-selected-model.wav"]);
    const pendingId = before.queue.jobs.find(
      (job) => job.audio_path === "/tmp/imported/missing-selected-model.wav",
    )?.id;
    expect(pendingId).toBeTypeOf("number");

    const processed = await processNextTranscription();

    expect(processed.queue.jobs.find((job) => job.id === pendingId)).toMatchObject({
      status: "pending",
      error: null,
    });

    await skipJob(pendingId ?? -1);
  });

  it("does not process browser fallback queue work while the queued model is unavailable", async () => {
    const current = await loadSnapshot();
    for (const job of current.queue.jobs.filter((candidate) => candidate.status === "pending")) {
      await skipJob(job.id);
    }

    await downloadModel("whisper-medium");
    await loadSnapshot();
    await saveSettingsPatch({ selected_model: "whisper-medium" });
    const queued = await enqueueAudioFiles(["/tmp/imported/missing-queued-model.wav"]);
    const pendingId = queued.queue.jobs.find(
      (job) => job.audio_path === "/tmp/imported/missing-queued-model.wav",
    )?.id;
    expect(pendingId).toBeTypeOf("number");

    await downloadModel("whisper-small");
    await loadSnapshot();
    await saveSettingsPatch({ selected_model: "whisper-small" });
    await deleteModel("whisper-medium");

    const processed = await processNextTranscription();

    expect(processed.queue.jobs.find((job) => job.id === pendingId)).toMatchObject({
      model_id: "whisper-medium",
      status: "pending",
      error: null,
    });

    await skipJob(pendingId ?? -1);
    await downloadModel("whisper-medium");
    await loadSnapshot();
    await saveSettingsPatch({ selected_model: "whisper-medium" });
    await deleteModel("whisper-small");
  });

  it("simulates model download and cancel state outside Tauri", async () => {
    const downloading = await downloadModel("whisper-small");
    expect(downloading.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const cancelled = await cancelModelDownload("whisper-small");
    expect(cancelled.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "error",
      download_progress: 0,
      download_error: "cancelled by user",
    });
  });

  it("cancels active browser fallback model downloads from the generic cancel action", async () => {
    const current = await loadSnapshot();
    const targetModelId =
      current.settings.selected_model === "whisper-small" ? "whisper-medium" : "whisper-small";
    await deleteModel(targetModelId);
    const downloading = await downloadModel(targetModelId);
    expect(downloading.models.find((model) => model.id === targetModelId)).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const cancelled = await cancelCurrentOperation();

    expect(cancelled.models.find((model) => model.id === targetModelId)).toMatchObject({
      status: "error",
      download_progress: 0,
      download_error: "cancelled by user",
    });
  });

  it("does not create cancelled errors for inactive browser fallback model downloads", async () => {
    await deleteModel("whisper-small");

    const cancelled = await cancelModelDownload("whisper-small");

    expect(cancelled.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "missing",
      download_progress: null,
      download_error: null,
    });
  });

  it("settles browser fallback model downloads during snapshot polling", async () => {
    await deleteModel("whisper-small");

    const downloading = await downloadModel("whisper-small");
    expect(downloading.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const settled = await loadSnapshot();

    expect(settled.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "ready",
      download_progress: 100,
      download_error: null,
    });
  });

  it("does not restart usable browser fallback model downloads", async () => {
    await deleteModel("whisper-small");
    await downloadModel("whisper-small");
    const ready = await loadSnapshot();
    expect(ready.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "ready",
      download_progress: 100,
      download_error: null,
    });

    const repeated = await downloadModel("whisper-small");

    expect(repeated.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "ready",
      download_progress: 100,
      download_error: null,
    });
  });

  it("does not delete active browser fallback model downloads", async () => {
    await deleteModel("whisper-small");
    const downloading = await downloadModel("whisper-small");
    expect(downloading.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const deleted = await deleteModel("whisper-small");

    expect(deleted.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });
  });

  it("does not verify active browser fallback model downloads", async () => {
    await deleteModel("whisper-small");
    const downloading = await downloadModel("whisper-small");
    expect(downloading.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });

    const verified = await verifyModel("whisper-small");

    expect(verified.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "downloading",
      download_progress: 0,
      download_error: null,
    });
  });

  it("settles browser fallback verification during snapshot polling", async () => {
    await downloadModel("whisper-small");
    await loadSnapshot();
    await saveSettingsPatch({ selected_model: "whisper-small" });
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
    await deleteModel("whisper-small");
    await downloadModel("whisper-small");
    await loadSnapshot();
    const selectedTiny = await saveSettingsPatch({ selected_model: "whisper-small" });
    expect(selectedTiny.settings.selected_model).toBe("whisper-small");

    await deleteModel("whisper-medium");
    const blocked = await saveSettingsPatch({ selected_model: "whisper-medium" });

    expect(blocked.settings.selected_model).toBe("whisper-small");
    expect(blocked.status.active_model).toBe("whisper-small");

    await downloadModel("whisper-medium");
    await loadSnapshot();
    const selectedMedium = await saveSettingsPatch({ selected_model: "whisper-medium" });

    expect(selectedMedium.settings.selected_model).toBe("whisper-medium");
    expect(selectedMedium.status.active_model).toBe("whisper-medium");
  });

  it("does not delete the active browser fallback model", async () => {
    await deleteModel("whisper-small");
    await downloadModel("whisper-small");
    await loadSnapshot();
    await saveSettingsPatch({ selected_model: "whisper-small" });

    const deleted = await deleteModel("whisper-small");

    expect(deleted.settings.selected_model).toBe("whisper-small");
    expect(deleted.status.active_model).toBe("whisper-small");
    expect(deleted.models.find((model) => model.id === "whisper-small")).toMatchObject({
      status: "ready",
      download_progress: 100,
      download_error: null,
    });
  });

  it("queues manually selected audio files outside Tauri", async () => {
    const before = await loadSnapshot();
    const snapshot = await enqueueAudioFiles([
      "/tmp/imported/meeting.wav",
      "/tmp/imported/call.m4a",
    ]);
    const audioPaths = snapshot.queue.jobs.map((job) => job.audio_path);

    expect(snapshot.queue.pending_count).toBe(before.queue.pending_count + 2);
    expect(audioPaths).toContain("/tmp/imported/meeting.wav");
    expect(audioPaths).toContain("/tmp/imported/call.m4a");
    expect(
      snapshot.queue.jobs
        .filter((job) => ["/tmp/imported/meeting.wav", "/tmp/imported/call.m4a"].includes(job.audio_path))
        .every((job) => job.model_id === snapshot.settings.selected_model),
    ).toBe(true);
  });

  it("does not duplicate manually imported browser fallback audio files", async () => {
    const audioPath = "/tmp/imported/duplicate-import.wav";

    await enqueueAudioFiles([audioPath]);
    const snapshot = await enqueueAudioFiles([audioPath]);

    expect(snapshot.queue.jobs.filter((job) => job.audio_path === audioPath)).toHaveLength(1);
  });

  it("requeues completed browser fallback audio for regeneration", async () => {
    const audioPath = "/tmp/imported/regenerate.wav";
    await enqueueAudioFiles([audioPath]);
    await processNextTranscription();

    const snapshot = await regenerateTranscript(audioPath);

    expect(snapshot.queue.jobs.find((job) => job.audio_path === audioPath)).toMatchObject({
      status: "pending",
      error: null,
    });
  });

  it("requeues browser fallback regeneration with an explicit model", async () => {
    await downloadModel("whisper-small");
    await loadSnapshot();
    await saveSettingsPatch({ selected_model: "whisper-medium" });
    const audioPath = "/tmp/imported/regenerate-with-tiny.wav";
    await enqueueAudioFiles([audioPath]);

    const snapshot = await regenerateTranscript(audioPath, "whisper-small");

    expect(snapshot.queue.jobs.find((job) => job.audio_path === audioPath)).toMatchObject({
      model_id: "whisper-small",
      status: "pending",
      error: null,
    });
  });

  it("no-ops browser fallback transcript folder opening", async () => {
    const snapshot = await openTranscriptFolder("/tmp/imported/regenerate-with-tiny.wav");

    expect(snapshot).toEqual(await loadSnapshot());
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

  it("does not process browser fallback queue work while another job is running", async () => {
    await downloadModel("whisper-medium");
    await loadSnapshot();
    await saveSettingsPatch({
      selected_model: "whisper-medium",
      transcription_enabled: true,
      pause_all: false,
    });
    const current = await loadSnapshot();
    for (const job of current.queue.jobs.filter((candidate) => candidate.status === "pending")) {
      await skipJob(job.id);
    }

    const queued = await enqueueAudioFiles([
      "/tmp/imported/single-flight-running.wav",
      "/tmp/imported/single-flight-pending.wav",
    ]);
    const runningJob = queued.queue.jobs.find(
      (candidate) => candidate.audio_path === "/tmp/imported/single-flight-running.wav",
    );
    const pendingJob = queued.queue.jobs.find(
      (candidate) => candidate.audio_path === "/tmp/imported/single-flight-pending.wav",
    );
    expect(runningJob?.id).toBeTypeOf("number");
    expect(pendingJob?.id).toBeTypeOf("number");
    if (runningJob) {
      runningJob.status = "running";
      queued.queue.pending_count -= 1;
      queued.queue.running_count += 1;
    }

    const processed = await processNextTranscription();

    expect(processed.queue.jobs.find((candidate) => candidate.id === runningJob?.id)).toMatchObject({
      status: "running",
      error: null,
    });
    expect(processed.queue.jobs.find((candidate) => candidate.id === pendingJob?.id)).toMatchObject({
      status: "pending",
      error: null,
    });

    await cancelCurrentTranscription();
    await skipJob(runningJob?.id ?? -1);
    await skipJob(pendingJob?.id ?? -1);
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

  it("cancels running browser fallback transcription jobs from the generic cancel action", async () => {
    await loadSnapshot();
    const before = await enqueueAudioFiles(["/tmp/imported/cancel-current-operation.wav"]);
    const job = before.queue.jobs.find(
      (candidate) => candidate.audio_path === "/tmp/imported/cancel-current-operation.wav",
    );
    expect(job?.id).toBeTypeOf("number");
    if (job) {
      job.status = "running";
      before.queue.pending_count -= 1;
      before.queue.running_count += 1;
    }

    const cancelled = await cancelCurrentOperation();

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

  it("does not allocate simulated browser capture sessions while recording is unavailable", async () => {
    await stopLiveCapture();
    await saveSettingsPatch({
      recording_enabled: true,
      transcription_enabled: true,
      pause_all: false,
      threshold_dbfs: -90,
    });

    await startLiveCapture();
    const synced = await stopLiveCapture();
    const syncedJob = synced.queue.jobs.at(-1);
    const syncedCaptureNumber = browserCaptureNumber(syncedJob?.audio_path);
    expect(syncedCaptureNumber).toBeTypeOf("number");

    await saveSettingsPatch({ recording_enabled: false, transcription_enabled: true });
    const blocked = await startLiveCapture();

    expect(blocked.status.live_input_active).toBe(false);
    expect(blocked.status.tray_state).toBe("idle");

    await saveSettingsPatch({ recording_enabled: true });
    await startLiveCapture();
    const stopped = await stopLiveCapture();
    const nextJob = stopped.queue.jobs.at(-1);

    expect(browserCaptureNumber(nextJob?.audio_path)).toBe((syncedCaptureNumber ?? 0) + 1);

    await skipJob(syncedJob?.id ?? -1);
    await skipJob(nextJob?.id ?? -1);
    await saveSettingsPatch({ threshold_dbfs: -45 });
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

  it("does not retroactively queue a simulated browser capture when transcription is enabled mid-recording", async () => {
    await stopLiveCapture();
    await saveSettingsPatch({
      recording_enabled: true,
      threshold_dbfs: -90,
      transcription_enabled: false,
    });
    const active = await startLiveCapture();
    const pendingBefore = active.queue.pending_count;

    await saveSettingsPatch({ transcription_enabled: true });
    const stopped = await stopLiveCapture();

    expect(stopped.status.live_input_active).toBe(false);
    expect(stopped.queue.pending_count).toBe(pendingBefore);

    await saveSettingsPatch({ threshold_dbfs: -45 });
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

describe("tauri source capture client (browser fallback)", () => {
  it("includes custom system-audio sources in browser fallback", async () => {
    await saveSettingsPatch({
      custom_sources: [
        {
          id: "spotify",
          label: "Spotify",
          title_patterns: ["Spotify"],
          auto_prompt: true,
        },
      ],
    });

    const sources = await loadRecognizedSources();

    expect(sources.some((source) => source.id === "spotify" && source.custom)).toBe(true);
  });

  it("flips simulated source capture state on start and stop (locked-behaviors §10)", async () => {
    const initial = await loadSourceCaptureStatus();
    expect(initial.capturing).toBe(false);

    await startSourceCapture("meet");
    const active = await loadSourceCaptureStatus();
    expect(active.capturing).toBe(true);
    expect(active.detected).toMatchObject({ source_id: "meet", label: "Google Meet" });

    await stopSourceCapture();
    const stopped = await loadSourceCaptureStatus();
    expect(stopped.capturing).toBe(false);
  });
});
