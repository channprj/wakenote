import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { QueueSnapshot } from "@/lib/types";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import { QueuePanel } from "./QueuePanel";

function renderQueue(queue: QueueSnapshot) {
  return renderToStaticMarkup(
    <QueuePanel
      queue={queue}
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
}

describe("QueuePanel compact layout", () => {
  it("preserves long values and labels every compact record field", () => {
    const markup = renderQueue({
      jobs: [
        {
          id: 9,
          audio_path: LONG_CONTENT.path,
          model_id: LONG_CONTENT.model,
          status: "failed",
          error: LONG_CONTENT.error,
        },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 1,
    });

    for (const label of ["Audio", "Model", "Status", "Actions"]) {
      expect(markup).toContain(`data-label="${label}"`);
    }
    expect(markup).toContain('data-slot="queue-summary"');
    expect(markup).toContain('data-slot="queue-table"');
    expect(markup).toContain(`title="${LONG_CONTENT.path}"`);
    expect(markup).toContain("overflow-wrap-anywhere");
    expect(markup).toContain(LONG_CONTENT.error);
  });

  it("does not leak danger tone into a clean queue", () => {
    const markup = renderQueue({
      jobs: [],
      pending_count: 0,
      running_count: 0,
      failed_count: 0,
    });

    expect(markup).not.toContain('data-tone="danger"');
  });

  it("offers one read action for unread failed, cancelled, and skipped outcomes", () => {
    const markup = renderQueue({
      jobs: [
        { id: 1, audio_path: "/failed.wav", model_id: "m", status: "failed" },
        { id: 2, audio_path: "/cancelled.wav", model_id: "m", status: "cancelled" },
        { id: 3, audio_path: "/skipped.wav", model_id: "m", status: "skipped" },
        { id: 4, audio_path: "/read.wav", model_id: "m", status: "failed", is_read: true },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 2,
    });

    expect(markup).toContain("Mark all read");
    expect(markup).toContain("Mark 3 outcomes as read");
    expect(markup).toMatch(/<button[^>]*title="Mark 3 outcomes as read"(?![^>]*disabled)/);
    expect(markup).toContain('data-read="true"');
  });

  it("disables mark all read when every attention outcome is already read", () => {
    const markup = renderQueue({
      jobs: [
        { id: 1, audio_path: "/read.wav", model_id: "m", status: "failed", is_read: true },
      ],
      pending_count: 0,
      running_count: 0,
      failed_count: 1,
    });

    expect(markup).toMatch(/<button[^>]*disabled=""[^>]*title="No unread outcomes"/);
  });
});
