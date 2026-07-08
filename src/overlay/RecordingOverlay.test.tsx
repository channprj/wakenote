import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { OverlayContent } from "./RecordingOverlay";
import { initialCaptionState, type OverlayCaptionState } from "./caption-state";

function caption(patch: Partial<OverlayCaptionState>): OverlayCaptionState {
  return {
    ...initialCaptionState(),
    ...patch,
  };
}

function render(state: OverlayCaptionState) {
  return renderToStaticMarkup(<OverlayContent caption={state} />);
}

describe("caption overlay content", () => {
  it("renders nothing before transcript text is available", () => {
    expect(render(caption({ chunkId: 1, visible: false }))).toBe("");
  });

  it("renders the live transcript as caption text without app controls or waveform", () => {
    const markup = render(
      caption({
        chunkId: 1,
        status: "partial",
        text: "회의에서 결정된 내용입니다",
        visible: true,
      }),
    );

    expect(markup).toContain('class="overlay-caption"');
    expect(markup).toContain('data-status="partial"');
    expect(markup).toContain("회의에서 결정된 내용입니다");
    expect(markup).not.toContain("overlay-waveform");
    expect(markup).not.toContain("lucide-mic");
    expect(markup).not.toContain("lucide-x");
    expect(markup).not.toContain("<button");
  });

  it("does not add visible status labels around final caption text", () => {
    const markup = render(
      caption({
        chunkId: 1,
        status: "final",
        text: "최종 전사 내용입니다",
        visible: true,
      }),
    );

    expect(markup).toContain("최종 전사 내용입니다");
    expect(markup).not.toContain("Final");
    expect(markup).not.toContain("Transcribing");
  });
});
