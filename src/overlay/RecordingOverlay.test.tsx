import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { OverlayContent } from "./RecordingOverlay";

function render(state: "hidden" | "recording" | "transcribing", levels?: number[]) {
  return renderToStaticMarkup(<OverlayContent state={state} levels={levels} onStop={() => {}} />);
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

  it("renders live waveform levels as fixed bar heights when recording", () => {
    const markup = render("recording", [0, 0.5, 1]);
    expect(markup.match(/class="overlay-waveform__bar"/g)?.length).toBe(11);
    expect(markup).toContain("--bar-height:4px");
    expect(markup).toContain("--bar-height:11px");
    expect(markup).toContain("--bar-height:18px");
    expect(markup).toContain('data-peak="true"');
    expect(markup).not.toContain("animation-delay");
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
