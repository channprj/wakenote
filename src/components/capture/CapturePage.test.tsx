import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import type { TranscriptEntry } from "@/lib/transcript-log";
import type { AppSnapshot } from "@/lib/types";
import type { ActivityAttention } from "@/lib/activity-attention";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import { CapturePage } from "./CapturePage";

function renderCapture(
  snapshot: AppSnapshot,
  transcriptEntries: TranscriptEntry[] = [],
  activityAttention: ActivityAttention | null = null,
) {
  return renderToStaticMarkup(
    <CapturePage
      snapshot={snapshot}
      activityAttention={activityAttention}
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
  it("renders recent transcription quality attention as a warning alert", () => {
    const markup = renderCapture(mockSnapshot(), [], {
      tone: "warning",
      count: 1,
      message: "1 transcription warning needs review",
      nextExpiryAt: Date.parse("2026-08-04T00:10:00.000Z"),
    });

    expect(markup).toContain('data-tone="warning"');
    expect(markup).toContain("1 transcription warning needs review");
    expect(markup).not.toContain('data-tone="danger"');
  });

  it("toggles Start/Stop Input and waveform state with live input", () => {
    const recording = mockSnapshot();
    recording.status.live_input_active = true;
    recording.status.tray_state = "recording";
    recording.status.level.current_dbfs = -22;
    recording.status.level.peak_dbfs = -8;

    const cases = [
      {
        label: "stopped",
        snapshot: mockSnapshot(),
        shown: ["Start Input", 'data-state="idle"'],
        hidden: ">Stop Input<",
      },
      {
        label: "recording",
        snapshot: recording,
        shown: ["Stop Input", 'data-state="recording"', "Current -22 dBFS"],
        hidden: ">Start Input<",
      },
    ];

    for (const { label, snapshot, shown, hidden } of cases) {
      const markup = renderCapture(snapshot);

      for (const text of shown) {
        expect(markup, label).toContain(text);
      }
      expect(markup, label).not.toContain(hidden);
    }
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
});
