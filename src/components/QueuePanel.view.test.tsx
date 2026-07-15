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
});
