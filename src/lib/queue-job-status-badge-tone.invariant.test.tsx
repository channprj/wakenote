// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { mockModels } from "./app-state";
import { queueJobStatusBadgeTone, queueStatsCellTone } from "./status-summary";
import { humanizeQueueJobStatus } from "./transcript-history";
import type { QueueJob, QueueJobStatus, QueueSnapshot } from "./types";
import { QueuePanel } from "../components/QueuePanel";

afterEach(cleanup);

// Cross-surface invariant: QueuePanel's per-row <tr> data-tone attribute and shadcn StatusBadge
// data-tone attribute must derive from the shared queueJobStatusBadgeTone() helper for the
// same QueueJobStatus value. Iter-73 promoted the helper to status-summary.ts with two co-located
// consumer surfaces in QueuePanel (line ~289 sets rowTone, line ~292 sets the Badge tone); this
// suite makes the intra-component cross-surface agreement EXPLICIT so a future regression where
// one surface drifts to a hardcoded tone literal fails a dedicated test rather than blending into
// per-component coverage. Mirrors iter-76's queueStatsCellTone invariant pattern (same component,
// two surfaces) and iter-71/74/77's cross-surface Badge tone invariants.
//
// Asymmetry note: the <tr> renders `data-tone={rowTone === "neutral" ? undefined : rowTone}`,
// so when the helper returns "neutral" the attribute is omitted from markup entirely. The
// invariant test normalizes absence-of-data-tone to "neutral" to compare against the Badge's
// (always-emitted) StatusBadge data-tone. This documented JSX contract on QueuePanel.tsx
// optimizes the DOM for the common "no signal" case while keeping the semantic tone derivable.
//
// The same render also checks QueuePanel's queue-stats banner cells and per-day group-row chip
// spans, which must derive their data-tone from the shared queueStatsCellTone() helper. Iter-75
// introduced queueStatsCellTone with those two co-located consumer surfaces; a regression where
// one reverts to a hardcoded data-tone literal (e.g. the pre-iter-75 `"warning"` / `"primary"` /
// `"danger"` / `"success"` string literals) fails here too.

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

function extractJobRowBadgeTone(
  container: HTMLElement,
  status: QueueJobStatus,
): string {
  const humanized = humanizeQueueJobStatus(status);
  const badge = Array.from(
    container.querySelectorAll<HTMLElement>('[data-slot="badge"]'),
  ).find((candidate) => candidate.textContent === humanized);
  expect(
    badge,
    `expected per-row Badge for status "${status}"`,
  ).toBeTruthy();
  return badge?.dataset.tone ?? "";
}

function extractJobRowDataTone(
  container: HTMLElement,
  status: QueueJobStatus,
): string {
  const humanized = humanizeQueueJobStatus(status);
  const badge = Array.from(
    container.querySelectorAll<HTMLElement>('[data-slot="badge"]'),
  ).find((candidate) => candidate.textContent === humanized);
  expect(
    badge,
    `expected per-row Badge for status "${status}" in markup`,
  ).toBeTruthy();
  const row = badge?.closest("tr");
  expect(
    row,
    `expected enclosing <tr for status "${status}"`,
  ).toBeTruthy();
  return row?.dataset.tone ?? "neutral";
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

describe("queue_job_status cross-surface tone invariant", () => {
  it("renders consistent QueuePanel tones on every surface for every queue job status", () => {
    const statuses: QueueJobStatus[] = [
      "pending",
      "running",
      "completed",
      "failed",
      "cancelled",
      "skipped",
    ];

    for (const status of statuses) {
      // Each status gets its own render so the "Visible N" button and Badge lookups stay unique.
      cleanup();
      const container = renderQueuePanel(status);

      // Row <tr> data-tone and StatusBadge data-tone share queueJobStatusBadgeTone.
      const expectedTone = queueJobStatusBadgeTone(status);
      const badgeTone = extractJobRowBadgeTone(container, status);
      const rowTone = extractJobRowDataTone(container, status);

      expect(badgeTone, `${status} badge`).toBe(expectedTone);
      expect(rowTone, `${status} row`).toBe(expectedTone);
      expect(badgeTone, `${status} badge vs row`).toBe(rowTone);

      // Queue-stats banner cell and per-day group-row chip share queueStatsCellTone.
      const expectedStatsTone = queueStatsCellTone(status);
      const statsCellTone = extractStatsCellTone(container, STATS_LABELS[status]);
      const groupChipTone = extractGroupChipTone(container, status);

      expect(statsCellTone, `${status} stats cell`).toBe(expectedStatsTone);
      expect(groupChipTone, `${status} group chip`).toBe(expectedStatsTone);
      expect(statsCellTone, `${status} stats cell vs group chip`).toBe(groupChipTone);
    }
  }, 30_000);
});
