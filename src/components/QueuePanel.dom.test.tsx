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

async function showAllStatuses(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("combobox", { name: "Filter by status" }));
  await user.click(screen.getByRole("option", { name: "All statuses" }));
}

describe("QueuePanel pagination", () => {
  it("renders content warnings separately from processing errors", () => {
    const nowMs = Date.parse("2026-08-04T12:00:00.000Z");
    const warningIssue = {
      severity: "warning" as const,
      code: "empty_transcript" as const,
      message: "OpenAI returned an empty transcript",
      occurred_at: new Date(nowMs - 1_000).toISOString(),
    };
    render(
      <QueuePanel
        nowMs={nowMs}
        queue={{
          jobs: [
            {
              id: 1,
              audio_path: "/empty.wav",
              model_id: "openai",
              status: "failed",
              issue: warningIssue,
            },
            {
              id: 2,
              audio_path: "/artifact.wav",
              model_id: "whisper",
              status: "completed",
              issue: {
                ...warningIssue,
                code: "transcript_artifact",
                message: "Transcript may contain an artifact",
              },
            },
            {
              id: 3,
              audio_path: "/auth.wav",
              model_id: "openai",
              status: "failed",
              issue: {
                severity: "error",
                code: "authentication",
                message: "Authentication failed",
                occurred_at: new Date(nowMs - 20 * 60_000).toISOString(),
              },
            },
          ],
          pending_count: 0,
          running_count: 0,
          failed_count: 2,
        }}
        models={[]}
        selectedModelId="whisper-medium"
        canProcessTranscription
        onImportAudioFiles={() => {}}
        onEnqueueBacklog={() => {}}
        onMarkAllRead={() => {}}
        onCancelCurrent={() => {}}
        onProcessNext={() => {}}
        onRetry={() => {}}
        onSkip={() => {}}
        onReprocess={async () => true}
      />,
    );

    const emptyRow = screen.getAllByTitle("/empty.wav")[0].closest("tr")!;
    const artifactRow = screen.getAllByTitle("/artifact.wav")[0].closest("tr")!;
    const authRow = screen.getAllByTitle("/auth.wav")[0].closest("tr")!;
    expect(within(emptyRow).getByText("Warning").dataset.tone).toBe("warning");
    expect(within(artifactRow).getByText("Completed").dataset.tone).toBe(
      "success",
    );
    expect(within(artifactRow).getByText("Warning").dataset.tone).toBe(
      "warning",
    );
    expect(within(authRow).getByText("Failed").dataset.tone).toBe("danger");
    expect(screen.getByText("Warnings").parentElement?.dataset.tone).toBe(
      "warning",
    );
    expect(screen.getByText("Errors").parentElement?.dataset.tone).toBe(
      "danger",
    );
  });

  it("moves between 50-item Activity pages without rendering the whole queue", async () => {
    const user = userEvent.setup();
    const jobs = paginatedJobs();
    render(
      <QueuePanel
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
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

    await showAllStatuses(user);

    expect(screen.getByText("1–50 of 51")).toBeTruthy();
    expect(
      document.querySelectorAll(
        '[data-slot="queue-table"] tbody > tr:not(.table-group-row)',
      ),
    ).toHaveLength(50);

    await user.click(
      screen.getByRole("button", { name: "Next Activity page" }),
    );

    expect(screen.getByText("51–51 of 51")).toBeTruthy();
    expect(
      document.querySelectorAll(
        '[data-slot="queue-table"] tbody > tr:not(.table-group-row)',
      ),
    ).toHaveLength(1);
    expect(
      document.querySelector('a[title="/recordings/20260802/000050.wav"]'),
    ).toBeTruthy();
  });

  it("requires confirmation before scanning the full backlog", async () => {
    const user = userEvent.setup();
    const onEnqueueBacklog = vi.fn();
    render(
      <QueuePanel
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
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

    const backlogButton = screen
      .getAllByRole("button", { name: "Process Backlog" })
      .at(-1)!;
    await user.click(backlogButton);

    expect(onEnqueueBacklog).not.toHaveBeenCalled();
    expect(
      screen.getByRole("alertdialog", { name: "Process the entire backlog?" }),
    ).toBeTruthy();
    expect(screen.getByText(/scan the full save folder/i)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onEnqueueBacklog).not.toHaveBeenCalled();

    await user.click(backlogButton);
    await user.click(
      screen.getByRole("button", { name: "Process entire backlog" }),
    );

    expect(onEnqueueBacklog).toHaveBeenCalledTimes(1);
  });

  it("switches between unresolved attention and resolved issue history", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <QueuePanel
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
        queue={{
          jobs: [
            {
              id: 1,
              audio_path: "/unread.wav",
              model_id: "m",
              status: "failed",
            },
            {
              id: 2,
              audio_path: "/resolved-failed.wav",
              model_id: "m",
              status: "failed",
              is_read: true,
            },
            {
              id: 3,
              audio_path: "/resolved-cancelled.wav",
              model_id: "m",
              status: "cancelled",
              is_read: true,
            },
            {
              id: 4,
              audio_path: "/completed.wav",
              model_id: "m",
              status: "completed",
            },
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

    expect(
      container.querySelector('a[title="/resolved-failed.wav"]'),
    ).toBeTruthy();
    expect(
      container.querySelector('a[title="/resolved-cancelled.wav"]'),
    ).toBeTruthy();
    expect(container.querySelector('a[title="/unread.wav"]')).toBeNull();
    expect(container.querySelector('a[title="/completed.wav"]')).toBeNull();

    await user.click(panel.getByRole("button", { name: "Needs attention 1" }));

    expect(container.querySelector('a[title="/unread.wav"]')).toBeTruthy();
    expect(
      container.querySelector('a[title="/resolved-failed.wav"]'),
    ).toBeNull();
  });

  it("hides clean completions by default and combines Activity filters", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <QueuePanel
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
        queue={{
          jobs: [
            {
              id: 1,
              audio_path: "/dictation-openai.wav",
              model_id: "openai",
              status: "failed",
              activity_kind: "dictation",
            },
            {
              id: 2,
              audio_path: "/live-openai.wav",
              model_id: "openai",
              status: "running",
              activity_kind: "live_transcription",
            },
            {
              id: 3,
              audio_path: "/live-local.wav",
              model_id: "local",
              status: "completed",
              activity_kind: "live_transcription",
            },
          ],
          pending_count: 0,
          running_count: 1,
          failed_count: 1,
        }}
        models={[]}
        selectedModelId="local"
        canProcessTranscription
        onImportAudioFiles={() => {}}
        onEnqueueBacklog={() => {}}
        onMarkAllRead={() => {}}
        onCancelCurrent={() => {}}
        onProcessNext={() => {}}
        onRetry={() => {}}
        onSkip={() => {}}
        onReprocess={async () => true}
      />,
    );

    expect(container.querySelector('a[title="/live-local.wav"]')).toBeNull();
    expect(
      container.querySelector('a[title="/dictation-openai.wav"]'),
    ).toBeTruthy();

    await user.click(
      screen.getByRole("combobox", { name: "Filter by AI model" }),
    );
    await user.click(screen.getByRole("option", { name: "openai" }));
    await user.click(
      screen.getByRole("combobox", { name: "Filter by transcription type" }),
    );
    await user.click(screen.getByRole("option", { name: "Dictation" }));
    await user.click(
      screen.getByRole("combobox", { name: "Filter by status" }),
    );
    await user.click(screen.getByRole("option", { name: "Failed" }));

    expect(
      container.querySelector('a[title="/dictation-openai.wav"]'),
    ).toBeTruthy();
    expect(container.querySelector('a[title="/live-openai.wav"]')).toBeNull();
    expect(screen.getByText("1 of 3 shown")).toBeTruthy();
  });

  it("selects every matching issue across pages and reprocesses with the chosen model", async () => {
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
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
        queue={{ jobs, pending_count: 0, running_count: 0, failed_count: 26 }}
        models={[
          readyFileModel("whisper-medium", "Whisper Medium"),
          readyFileModel("whisper-small", "Whisper Small"),
          {
            ...readyFileModel("openai", "OpenAI Transcribe"),
            offline: false,
            provider_runtime: "openai-stt",
          },
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
    const selectAll = panel.getByRole("checkbox", {
      name: "Select all 51 matching items",
    });
    expect(selectAll.closest("th")).toBeTruthy();
    expect(panel.queryByText("Select all")).toBeNull();
    await user.click(selectAll);
    expect(panel.getByText("51 selected")).toBeTruthy();
    expect(panel.getByRole("button", { name: "Reprocess 51" })).toBeTruthy();

    await user.click(
      panel.getByRole("combobox", { name: "Reprocessing model" }),
    );
    expect(screen.getByText("Local models")).toBeTruthy();
    expect(screen.getByText("API models")).toBeTruthy();
    await user.click(screen.getByRole("option", { name: "Whisper Small" }));
    await user.click(panel.getByRole("button", { name: "Reprocess 51" }));

    expect(onReprocess).toHaveBeenCalledTimes(1);
    expect(onReprocess).toHaveBeenCalledWith(
      Array.from({ length: 51 }, (_, index) => index + 1),
      "whisper-small",
    );
  }, 15_000);

  it("supports selecting only one issue", async () => {
    const user = userEvent.setup();
    const onReprocess = vi.fn().mockResolvedValue(true);
    render(
      <QueuePanel
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
        queue={{
          jobs: [
            {
              id: 1,
              audio_path: "/one.wav",
              model_id: "old",
              status: "failed",
            },
            {
              id: 2,
              audio_path: "/two.wav",
              model_id: "old",
              status: "cancelled",
            },
            {
              id: 3,
              audio_path: "/done.wav",
              model_id: "old",
              status: "completed",
            },
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

    await user.click(screen.getByRole("checkbox", { name: "Select one.wav" }));
    await user.click(screen.getByRole("button", { name: "Reprocess 1" }));

    expect(onReprocess).toHaveBeenCalledWith([1], "whisper-small");
  });

  it("confirms before moving every selected non-running Activity item to Trash", async () => {
    const user = userEvent.setup();
    const onTrash = vi.fn().mockResolvedValue([1, 2]);
    render(
      <QueuePanel
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
        queue={{
          jobs: [
            {
              id: 1,
              audio_path: "/done.wav",
              model_id: "old",
              status: "completed",
            },
            {
              id: 2,
              audio_path: "/failed.wav",
              model_id: "old",
              status: "failed",
            },
            {
              id: 3,
              audio_path: "/active.wav",
              model_id: "old",
              status: "running",
            },
          ],
          pending_count: 0,
          running_count: 1,
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
        onTrash={onTrash}
        onReprocess={async () => true}
      />,
    );

    await showAllStatuses(user);

    await user.click(
      screen.getByRole("checkbox", { name: "Select all 2 matching items" }),
    );
    expect(
      screen.queryByRole("checkbox", { name: "Select active.wav" }),
    ).toBeNull();

    await user.click(screen.getByRole("button", { name: "Move 2 to Trash" }));

    expect(onTrash).not.toHaveBeenCalled();
    expect(
      screen.getByRole("alertdialog", {
        name: "Move 2 recordings to Trash?",
      }),
    ).toBeTruthy();
    expect(screen.getByText(/Activity records will be removed/i)).toBeTruthy();

    await user.click(
      screen.getByRole("button", { name: "Move recordings to Trash" }),
    );

    expect(onTrash).toHaveBeenCalledWith([1, 2]);
  });

  it("plays an Activity item in one shared dock and opens its containing folder", async () => {
    const user = userEvent.setup();
    const onOpenFolder = vi.fn();
    render(
      <QueuePanel
        nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
        queue={{
          jobs: [
            {
              id: 7,
              audio_path: "/recordings/20260804/010203.m4a",
              model_id: "whisper-small",
              status: "completed",
            },
          ],
          pending_count: 0,
          running_count: 0,
          failed_count: 0,
        }}
        models={[]}
        selectedModelId="whisper-small"
        canProcessTranscription
        onImportAudioFiles={() => {}}
        onEnqueueBacklog={() => {}}
        onMarkAllRead={() => {}}
        onCancelCurrent={() => {}}
        onProcessNext={() => {}}
        onRetry={() => {}}
        onSkip={() => {}}
        onOpenFolder={onOpenFolder}
        onReprocess={async () => true}
      />,
    );

    await showAllStatuses(user);

    await user.click(
      screen.getByRole("button", { name: "Play audio: 010203.m4a" }),
    );

    const player = screen.getByRole("complementary", {
      name: "Activity audio player",
    });
    expect(player.querySelector("audio")?.getAttribute("src")).toBe(
      "file:///recordings/20260804/010203.m4a",
    );
    expect(within(player).getByText("010203.m4a")).toBeTruthy();

    await user.click(
      screen.getByRole("button", { name: "Show in Finder: 010203.m4a" }),
    );
    expect(onOpenFolder).toHaveBeenCalledWith(
      "/recordings/20260804/010203.m4a",
    );
  });
});
