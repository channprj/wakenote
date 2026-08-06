import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockModels } from "./app-state";
import { queueJobStatusBadgeTone } from "./status-summary";
import { humanizeQueueJobStatus } from "./transcript-history";
import type { QueueJob, QueueJobStatus, QueueSnapshot } from "./types";
import { QueuePanel } from "../components/QueuePanel";

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

function renderQueuePanelMarkup(status: QueueJobStatus): string {
  return renderToStaticMarkup(
    <QueuePanel
      nowMs={Date.parse("2026-08-04T12:00:00.000Z")}
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
}

function extractJobRowBadgeTone(
  markup: string,
  status: QueueJobStatus,
): string {
  const humanized = humanizeQueueJobStatus(status);
  const match = markup.match(
    new RegExp(`<span[^>]*data-tone="([a-z]+)"[^>]*>${humanized}<\\/span>`),
  );
  expect(match, `expected per-row Badge for status "${status}"`).not.toBeNull();
  return match?.[1] ?? "";
}

function extractJobRowDataTone(markup: string, status: QueueJobStatus): string {
  // The job row is the <tr> that contains the per-row Badge — locate it by scanning backward
  // from the Badge's position to the nearest preceding `<tr` opening tag, then parse data-tone
  // from that tag's attributes (returning "neutral" if the attribute is omitted, per the
  // documented `rowTone === "neutral" ? undefined : rowTone` JSX contract on QueuePanel.tsx:295).
  // Anchors on the unique StatusBadge data-tone signature so the queue-stats banner label spans
  // (e.g. `<span>Completed</span>`) don't collide with StatusBadge text.
  const humanized = humanizeQueueJobStatus(status);
  const badgePattern = new RegExp(
    `<span[^>]*data-tone="[a-z]+"[^>]*>${humanized}<\\/span>`,
  );
  const badgeMatch = badgePattern.exec(markup);
  expect(
    badgeMatch,
    `expected per-row Badge for status "${status}" in markup`,
  ).not.toBeNull();
  const badgeIdx = badgeMatch?.index ?? -1;
  const trOpenIdx = markup.lastIndexOf("<tr", badgeIdx);
  expect(
    trOpenIdx,
    `expected enclosing <tr for status "${status}"`,
  ).toBeGreaterThan(-1);
  const trEndIdx = markup.indexOf(">", trOpenIdx);
  const trAttrs = markup.slice(trOpenIdx, trEndIdx + 1);
  const match = trAttrs.match(/data-tone="([a-z]+)"/);
  return match ? match[1] : "neutral";
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
      const markup = renderQueuePanelMarkup(status);
      const expectedTone = queueJobStatusBadgeTone(status);

      const badgeTone = extractJobRowBadgeTone(markup, status);
      const rowTone = extractJobRowDataTone(markup, status);

      expect(badgeTone).toBe(expectedTone);
      expect(rowTone).toBe(expectedTone);
      expect(badgeTone).toBe(rowTone);
    });
  },
);
