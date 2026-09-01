// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { mockModels } from "./app-state";
import { queueStatsCellTone } from "./status-summary";
import type { QueueJob, QueueJobStatus, QueueSnapshot } from "./types";
import { QueuePanel } from "../components/QueuePanel";

afterEach(cleanup);

// Cross-surface invariant: QueuePanel's queue-stats banner cells and per-day group-row chip spans
// must derive their data-tone from the shared queueStatsCellTone() helper for the same
// QueueJobStatus value. Iter-75 introduced queueStatsCellTone with two co-located consumer
// surfaces; this suite makes the cross-surface agreement EXPLICIT so a future regression where
// one surface reverts to a hardcoded data-tone literal (e.g. the pre-iter-75 `"warning"` /
// `"primary"` / `"danger"` / `"success"` string literals) fails an additional dedicated test
// rather than just looking like an isolated per-component failure. Mirrors the iter-71 tray-state
// and iter-74 model-status invariant suites.

const STATS_LABELS: Record<QueueJobStatus, string> = {
  pending: "Pending",
  running: "Running",
  completed: "Completed",
  failed: "Errors",
  cancelled: "Cancelled",
  skipped: "Skipped",
};

function buildQueueSnapshot(status: QueueJobStatus): QueueSnapshot {
  const job: QueueJob = {
    id: 1,
    audio_path: "/tmp/wakenote/20260101/120000.wav",
    model_id: "whisper-medium",
    status,
    error: null,
    issue:
      status === "completed"
        ? {
            severity: "warning",
            code: "transcript_artifact",
            message: "Review transcript",
          }
        : null,
  };
  return {
    jobs: [job],
    pending_count: status === "pending" ? 1 : 0,
    running_count: status === "running" ? 1 : 0,
    failed_count: status === "failed" ? 1 : 0,
  };
}

function renderQueuePanel(status: QueueJobStatus): HTMLElement {
  const { container } = render(
    <QueuePanel
      nowMs={Date.parse("2026-01-01T12:00:00.000Z")}
      queue={buildQueueSnapshot(status)}
      models={mockModels()}
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
  fireEvent.click(
    screen.getByRole("button", { name: /^Visible \d+$/ }),
  );
  return container;
}

function extractStatsCellTone(container: HTMLElement, label: string): string {
  const cell = Array.from(
    container.querySelectorAll<HTMLElement>(
      '[data-slot="queue-summary"] > div',
    ),
  ).find((candidate) => candidate.querySelector("span")?.textContent === label);
  expect(
    cell,
    `expected queue-stats banner cell labeled "${label}"`,
  ).toBeTruthy();
  return cell?.dataset.tone ?? "";
}

function extractGroupChipTone(
  container: HTMLElement,
  status: QueueJobStatus,
): string {
  const bucket = status === "failed" ? "error" : status;
  const chip = Array.from(
    container.querySelectorAll<HTMLElement>(".table-group-row span[data-tone]"),
  ).find((candidate) => candidate.textContent?.endsWith(` ${bucket}`));
  expect(
    chip,
    `expected per-day group-row chip for "${status}"`,
  ).toBeTruthy();
  return chip?.dataset.tone ?? "";
}

describe.each<QueueJobStatus>([
  "pending",
  "running",
  "completed",
  "failed",
  "cancelled",
  "skipped",
])("queue_job_status data-tone cross-surface invariant for %s", (status) => {
  it("renders the same data-tone in queue-stats banner cell and per-day group-row chip", () => {
    const container = renderQueuePanel(status);
    const expectedTone = queueStatsCellTone(status);

    const statsCellTone = extractStatsCellTone(container, STATS_LABELS[status]);
    const groupChipTone = extractGroupChipTone(container, status);

    expect(statsCellTone).toBe(expectedTone);
    expect(groupChipTone).toBe(expectedTone);
    expect(statsCellTone).toBe(groupChipTone);
  });
});
