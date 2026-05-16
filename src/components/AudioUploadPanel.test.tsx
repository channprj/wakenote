import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AudioUploadPanel } from "./AudioUploadPanel";

describe("AudioUploadPanel", () => {
  it("renders upload controls without the stored path pattern", () => {
    const markup = renderToStaticMarkup(<AudioUploadPanel />);

    expect(markup).toContain("Upload Audio");
    expect(markup).not.toContain("Pattern");
    expect(markup).not.toContain("{original_filename}");
  });

  it("renders overview waveform, zoomed waveform, transport, and skip silence controls", () => {
    const markup = renderToStaticMarkup(
      <AudioUploadPanel
        initialAudio={{
          audio_path: "/tmp/WakeNote/uploaded/20260516/meeting.mp3",
          original_filename: "meeting.mp3",
          stored_at: "2026-05-16T12:00:00+09:00",
        }}
        initialWaveform={{
          duration_seconds: 120,
          sample_rate: 8000,
          peaks: [0.1, 0.4, 0.2, 1, 0.3, 0.7],
          audible_ranges: [
            { start: 4, end: 30 },
            { start: 40, end: 80 },
          ],
        }}
      />,
    );

    expect(markup).toContain("audio-waveform-overview");
    expect(markup).toContain("audio-waveform-detail");
    expect(markup).toContain("Zoom");
    expect(markup).toContain("Skip Silence");
    expect(markup).toContain('src="file:///tmp/WakeNote/uploaded/20260516/meeting.mp3"');
  });

  it("renders the macOS-style transport with skip buttons and time readout", () => {
    const markup = renderToStaticMarkup(
      <AudioUploadPanel
        initialAudio={{
          audio_path: "/tmp/sample.mp3",
          original_filename: "sample.mp3",
          stored_at: "2026-05-16T12:00:00+09:00",
        }}
        initialWaveform={{
          duration_seconds: 90,
          sample_rate: 8000,
          peaks: [0.2, 0.6, 0.1, 0.9],
          audible_ranges: [{ start: 1, end: 80 }],
        }}
      />,
    );

    expect(markup).toContain("audio-transport");
    expect(markup).toContain("Skip back 10 seconds");
    expect(markup).toContain("Skip back 5 seconds");
    expect(markup).toContain("Skip forward 5 seconds");
    expect(markup).toContain("Skip forward 10 seconds");
    expect(markup).toContain('aria-label="Play"');
    expect(markup).toContain("1:30");
  });
});
