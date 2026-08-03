// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
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
});
