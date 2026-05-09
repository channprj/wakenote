import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { OverlayContent } from "./RecordingOverlay";

function render(state: "hidden" | "recording" | "transcribing") {
  return renderToStaticMarkup(<OverlayContent state={state} onStop={() => {}} />);
}

describe("recording overlay states", () => {
  it("renders nothing when hidden", () => {
    expect(render("hidden")).toBe("");
  });

  it("renders mic icon, waveform bars, and close X button when recording", () => {
    const markup = render("recording");
    expect(markup).toContain('data-state="recording"');
    expect(markup).toContain("lucide-mic");
    expect(markup).toContain("overlay-waveform");
    expect(markup).toContain('aria-label="Stop recording"');
    expect(markup).toContain("lucide-x");
  });

  it("renders Loader2 spinner and Transcribing label when transcribing", () => {
    const markup = render("transcribing");
    expect(markup).toContain('data-state="transcribing"');
    // lucide-react renders Loader2 with class "lucide-loader-circle"
    expect(markup).toContain("lucide-loader-circle");
    expect(markup).toContain("Transcribing");
    expect(markup).toContain("overlay-pill__middle--pulse");
  });
});
