import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import type { TranscriptEntry } from "@/lib/transcript-log";
import type { AppSnapshot } from "@/lib/types";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
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

    expect(markup).toContain(
      "Primary input &quot;System Default&quot; is unavailable. WakeNote will wait for the same device.",
    );
    expect(markup).toContain('title="No microphone available"');
  });

  it("keeps a long Korean transcript in the page flow", () => {
    const snapshot = mockSnapshot();
    const transcript: TranscriptEntry = {
      chunk_id: 7,
      status: "partial",
      text: LONG_CONTENT.korean,
      started_at: "2026-07-15T00:00:00Z",
      recorded_at: "2026-07-15T00:00:00Z",
      audio_path: null,
      error: null,
    };

    expect(renderCapture(snapshot, [transcript])).toContain(
      LONG_CONTENT.korean,
    );
  });

  it("explains when live text is provisional and final transcription stays selected", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.selected_model = "qwen3-asr-1.7b";
    snapshot.status.active_model = "qwen3-asr-1.7b";
    snapshot.models.find((model) => model.id === "qwen3-asr-1.7b")!.status =
      "ready";
    snapshot.models.find((model) => model.id === "whisper-turbo")!.status =
      "ready";

    const markup = renderCapture(snapshot);

    expect(markup).toContain(
      "Live preview uses Whisper Turbo; final transcription remains Qwen3-ASR 1.7B.",
    );
  });

  it("explains final-only waiting when no preview model is ready", () => {
    const snapshot = mockSnapshot();
    snapshot.settings.selected_model = "openrouter-qwen3-asr-flash";
    snapshot.status.active_model = "openrouter-qwen3-asr-flash";
    snapshot.models.find(
      (model) => model.id === "openrouter-qwen3-asr-flash",
    )!.status = "ready";

    const markup = renderCapture(snapshot);

    expect(markup).toContain(
      "Live preview waits for a compatible model; final transcription uses OpenRouter · Qwen3 ASR Flash.",
    );
  });
});
