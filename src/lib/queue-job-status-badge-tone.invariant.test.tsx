// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { mockModels } from "./app-state";
import { queueJobStatusBadgeTone } from "./status-summary";
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

describe.each<QueueJobStatus>([
  "pending",
  "running",
  "completed",
  "failed",
  "cancelled",
  "skipped",
])(
  "queue_job_status Badge/row-data-tone cross-surface invariant for %s",
  (status) => {
    it("renders the same effective tone for row and StatusBadge data-tone", () => {
      const container = renderQueuePanel(status);
      const expectedTone = queueJobStatusBadgeTone(status);

      const badgeTone = extractJobRowBadgeTone(container, status);
      const rowTone = extractJobRowDataTone(container, status);

      expect(badgeTone).toBe(expectedTone);
      expect(rowTone).toBe(expectedTone);
      expect(badgeTone).toBe(rowTone);
    });
  },
);
