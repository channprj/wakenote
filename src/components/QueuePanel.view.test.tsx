import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { QueueSnapshot } from "@/lib/types";
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
  it("preserves full long paths and wraps long errors", () => {
    const audioPath = `/Volumes/990EVO+/workspace/chann/wakenote/${"nested/".repeat(15)}clip.m4a`;
    const error = `Runtime stream failed: ${"device-disconnected/".repeat(12)}`;
    const markup = renderQueue({
      jobs: [{ id: 9, audio_path: audioPath, model_id: "model/long-name", status: "failed", error }],
      pending_count: 0,
      running_count: 0,
      failed_count: 1,
    });

    expect(markup).toContain('data-slot="queue-summary"');
    expect(markup).toContain('data-slot="queue-table"');
    expect(markup).toContain(`title="${audioPath}"`);
    expect(markup).toContain("overflow-wrap-anywhere");
    expect(markup).toContain(error);
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
