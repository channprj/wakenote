import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import type { TranscriptEntry } from "@/lib/transcript-log";
import type { AppSnapshot } from "@/lib/types";
import { CapturePage } from "./CapturePage";

function renderCapture(
  snapshot: AppSnapshot,
  transcriptEntries: TranscriptEntry[] = [],
) {
  return renderToStaticMarkup(
    <CapturePage
      snapshot={snapshot}
      transcriptEntries={transcriptEntries}
      busy={false}
      onStart={() => {}}
      onStop={() => {}}
      onRefresh={() => {}}
      onPatch={() => {}}
      onOpenAudioSettings={() => {}}
    />,
  );
}

describe("CapturePage", () => {
  it("shows only Start Input while capture is stopped", () => {
    const markup = renderCapture(mockSnapshot());

    expect(markup).toContain("Start Input");
    expect(markup).not.toContain(">Stop Input<");
    expect(markup).toContain('data-state="idle"');
  });

  it("shows Stop Input and recording waveform state while recording", () => {
    const snapshot = mockSnapshot();
    snapshot.status.live_input_active = true;
    snapshot.status.tray_state = "recording";
    snapshot.status.level.current_dbfs = -22;
    snapshot.status.level.peak_dbfs = -8;

    const markup = renderCapture(snapshot);

    expect(markup).toContain("Stop Input");
    expect(markup).not.toContain(">Start Input<");
    expect(markup).toContain('data-state="recording"');
    expect(markup).toContain("Current -22 dBFS");
  });

  it("surfaces missing microphone guidance and the exact disabled reason", () => {
    const snapshot = mockSnapshot();
    snapshot.microphones = [];

    const markup = renderCapture(snapshot);

    expect(markup).toContain("No available input device is selected.");
    expect(markup).toContain('title="No microphone available"');
  });

  it("keeps a long Korean transcript in the page flow", () => {
    const snapshot = mockSnapshot();
    const transcript: TranscriptEntry = {
      chunk_id: 7,
      status: "partial",
      text: "길이가 매우 긴 한국어 전사도 카드 경계를 벗어나지 않고 전체 내용을 보여줍니다.",
      started_at: "2026-07-15T00:00:00Z",
      recorded_at: "2026-07-15T00:00:00Z",
      audio_path: null,
      error: null,
    };

    expect(renderCapture(snapshot, [transcript])).toContain("길이가 매우 긴 한국어 전사");
  });
});
