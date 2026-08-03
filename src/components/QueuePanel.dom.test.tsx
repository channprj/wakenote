// @vitest-environment jsdom

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { QueueJob } from "../lib/types";
import { QueuePanel } from "./QueuePanel";

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
});
