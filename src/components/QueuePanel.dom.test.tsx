// @vitest-environment jsdom

import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ModelDescriptor, QueueJob } from "../lib/types";
import { QueuePanel } from "./QueuePanel";

afterEach(cleanup);

Object.defineProperties(HTMLElement.prototype, {
  hasPointerCapture: {
    configurable: true,
    value: () => false,
  },
  setPointerCapture: {
    configurable: true,
    value: () => {},
  },
  releasePointerCapture: {
    configurable: true,
    value: () => {},
  },
  scrollIntoView: {
    configurable: true,
    value: () => {},
  },
});

function readyFileModel(id: string, displayName: string): ModelDescriptor {
  return {
    id,
    display_name: displayName,
    engine: "test",
    provider_runtime: "test",
    size_mb: 1,
    languages: ["en"],
    speed_score: 1,
    accuracy_score: 1,
    offline: true,
    status: "ready",
    capabilities: {
      file_transcription: true,
      realtime: false,
      streaming: "unsupported",
      diarization: false,
      cost_reporting: "none",
      maximum_request_bytes: null,
      selectable_contexts: ["file"],
    },
  };
}

function paginatedJobs(): QueueJob[] {
  const olderJobs: QueueJob[] = Array.from({ length: 50 }, (_, index) => ({
    id: index + 1,
    audio_path: `/recordings/20260802/${String(index + 1).padStart(6, "0")}.wav`,
    model_id: "whisper-medium",
    status: "completed",
  }));
  return [
    ...olderJobs,
    {
      id: 51,
      audio_path: "/recordings/20260803/010203.wav",
      model_id: "whisper-medium",
      status: "failed",
    },
  ];
}

describe("QueuePanel pagination", () => {
  it("moves between 50-item Activity pages without rendering the whole queue", async () => {
    const user = userEvent.setup();
    const jobs = paginatedJobs();
    render(
      <QueuePanel
        queue={{
          jobs,
          pending_count: 0,
          running_count: 0,
          failed_count: 1,
        }}
        models={[]}
        canProcessTranscription
        onImportAudioFiles={() => {}}
        onEnqueueBacklog={() => {}}
        onMarkAllRead={() => {}}
        onCancelCurrent={() => {}}
        onProcessNext={() => {}}
        onRetry={() => {}}
        onSkip={() => {}}
        selectedModelId="whisper-medium"
        onReprocess={async () => true}
      />,
    );

    expect(screen.getByText("1–50 of 51")).toBeTruthy();
    expect(document.querySelectorAll('[data-slot="queue-table"] tbody > tr:not(.table-group-row)')).toHaveLength(50);

    await user.click(screen.getByRole("button", { name: "Next Activity page" }));

    expect(screen.getByText("51–51 of 51")).toBeTruthy();
    expect(document.querySelectorAll('[data-slot="queue-table"] tbody > tr:not(.table-group-row)')).toHaveLength(1);
    expect(document.querySelector('a[title="/recordings/20260802/000050.wav"]')).toBeTruthy();
  });

  it("requires confirmation before scanning the full backlog", async () => {
    const user = userEvent.setup();
    const onEnqueueBacklog = vi.fn();
    render(
      <QueuePanel
        queue={{
          jobs: [],
          pending_count: 0,
          running_count: 0,
          failed_count: 0,
        }}
        models={[]}
        canProcessTranscription
        onImportAudioFiles={() => {}}
        onEnqueueBacklog={onEnqueueBacklog}
        onMarkAllRead={() => {}}
        onCancelCurrent={() => {}}
        onProcessNext={() => {}}
        onRetry={() => {}}
        onSkip={() => {}}
        selectedModelId="whisper-medium"
        onReprocess={async () => true}
      />,
    );

    const backlogButton = screen.getAllByRole("button", { name: "Process Backlog" }).at(-1)!;
    await user.click(backlogButton);

    expect(onEnqueueBacklog).not.toHaveBeenCalled();
    expect(screen.getByRole("alertdialog", { name: "Process the entire backlog?" })).toBeTruthy();
    expect(screen.getByText(/scan the full save folder/i)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onEnqueueBacklog).not.toHaveBeenCalled();

    await user.click(backlogButton);
    await user.click(screen.getByRole("button", { name: "Process entire backlog" }));

    expect(onEnqueueBacklog).toHaveBeenCalledTimes(1);
  });

  it("switches between unresolved attention and resolved issue history", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <QueuePanel
        queue={{
          jobs: [
            { id: 1, audio_path: "/unread.wav", model_id: "m", status: "failed" },
            { id: 2, audio_path: "/resolved-failed.wav", model_id: "m", status: "failed", is_read: true },
            { id: 3, audio_path: "/resolved-cancelled.wav", model_id: "m", status: "cancelled", is_read: true },
            { id: 4, audio_path: "/completed.wav", model_id: "m", status: "completed" },
          ],
          pending_count: 0,
          running_count: 0,
          failed_count: 2,
        }}
        models={[]}
        canProcessTranscription
        onImportAudioFiles={() => {}}
        onEnqueueBacklog={() => {}}
        onMarkAllRead={() => {}}
        onCancelCurrent={() => {}}
        onProcessNext={() => {}}
        onRetry={() => {}}
        onSkip={() => {}}
        selectedModelId="whisper-medium"
        onReprocess={async () => true}
      />,
    );
    const panel = within(container);

    await user.click(panel.getByRole("button", { name: "Resolved 2" }));

    expect(container.querySelector('a[title="/resolved-failed.wav"]')).toBeTruthy();
    expect(container.querySelector('a[title="/resolved-cancelled.wav"]')).toBeTruthy();
    expect(container.querySelector('a[title="/unread.wav"]')).toBeNull();
    expect(container.querySelector('a[title="/completed.wav"]')).toBeNull();

    await user.click(panel.getByRole("button", { name: "Needs attention 1" }));

    expect(container.querySelector('a[title="/unread.wav"]')).toBeTruthy();
    expect(container.querySelector('a[title="/resolved-failed.wav"]')).toBeNull();
  });

  it(
    "selects every matching issue across pages and reprocesses with the chosen model",
    async () => {
      const user = userEvent.setup();
      const onReprocess = vi.fn().mockResolvedValue(true);
      const jobs: QueueJob[] = Array.from({ length: 51 }, (_, index) => ({
        id: index + 1,
        audio_path: `/recordings/20260803/${String(index + 1).padStart(6, "0")}.wav`,
        model_id: "whisper-medium",
        status: index % 2 === 0 ? "failed" : "skipped",
        is_read: index > 24,
      }));
      const { container } = render(
        <QueuePanel
          queue={{ jobs, pending_count: 0, running_count: 0, failed_count: 26 }}
          models={[
            readyFileModel("whisper-medium", "Whisper Medium"),
            readyFileModel("whisper-small", "Whisper Small"),
          ]}
          selectedModelId="whisper-medium"
          canProcessTranscription
          onImportAudioFiles={() => {}}
          onEnqueueBacklog={() => {}}
          onMarkAllRead={() => {}}
          onCancelCurrent={() => {}}
          onProcessNext={() => {}}
          onRetry={() => {}}
          onSkip={() => {}}
          onReprocess={onReprocess}
        />,
      );

      const panel = within(container);
      await user.click(
        panel.getByRole("checkbox", { name: "Select all 51 matching issues" }),
      );
      expect(panel.getByText("51 selected")).toBeTruthy();
      expect(panel.getByRole("button", { name: "Reprocess 51" })).toBeTruthy();

      await user.click(panel.getByRole("combobox", { name: "Reprocessing model" }));
      await user.click(screen.getByRole("option", { name: "Whisper Small" }));
      await user.click(panel.getByRole("button", { name: "Reprocess 51" }));

      expect(onReprocess).toHaveBeenCalledTimes(1);
      expect(onReprocess).toHaveBeenCalledWith(
        Array.from({ length: 51 }, (_, index) => index + 1),
        "whisper-small",
      );
    },
    15_000,
  );

  it("supports selecting only one issue", async () => {
    const user = userEvent.setup();
    const onReprocess = vi.fn().mockResolvedValue(true);
    render(
      <QueuePanel
        queue={{
          jobs: [
            { id: 1, audio_path: "/one.wav", model_id: "old", status: "failed" },
            { id: 2, audio_path: "/two.wav", model_id: "old", status: "cancelled" },
            { id: 3, audio_path: "/done.wav", model_id: "old", status: "completed" },
          ],
          pending_count: 0,
          running_count: 0,
          failed_count: 1,
        }}
        models={[readyFileModel("whisper-small", "Whisper Small")]}
        selectedModelId="whisper-small"
        canProcessTranscription
        onImportAudioFiles={() => {}}
        onEnqueueBacklog={() => {}}
        onMarkAllRead={() => {}}
        onCancelCurrent={() => {}}
        onProcessNext={() => {}}
        onRetry={() => {}}
        onSkip={() => {}}
        onReprocess={onReprocess}
      />,
    );

    await user.click(
      screen.getByRole("checkbox", { name: "Select one.wav for reprocessing" }),
    );
    await user.click(screen.getByRole("button", { name: "Reprocess 1" }));

    expect(onReprocess).toHaveBeenCalledWith([1], "whisper-small");
  });
});
