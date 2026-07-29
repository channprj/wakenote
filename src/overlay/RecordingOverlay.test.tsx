import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  OverlayContent,
  emptyCaptionSnapshot,
  type DictationOverlaySnapshot,
  type OverlayCaptionSnapshot,
} from "./RecordingOverlay";

const hiddenDictation: DictationOverlaySnapshot = {
  state: "hidden",
  message: null,
};

function caption(patch: Partial<OverlayCaptionSnapshot>): OverlayCaptionSnapshot {
  return {
    ...emptyCaptionSnapshot(),
    ...patch,
  };
}

function render(state: OverlayCaptionSnapshot) {
  return renderToStaticMarkup(
    <OverlayContent
      caption={state}
      dictation={hiddenDictation}
      levels={[]}
      elapsedSeconds={0}
    />,
  );
}

describe("caption overlay content", () => {
  it("renders nothing before transcript text is available", () => {
    expect(render(caption({ chunk_id: 1, visible: false }))).toBe("");
  });

  it("renders the live transcript as caption text without app controls or waveform", () => {
    const markup = render(
      caption({
        chunk_id: 1,
        phase: "partial",
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
        chunk_id: 1,
        phase: "final",
        text: "최종 전사 내용입니다",
        visible: true,
      }),
    );

    expect(markup).toContain("최종 전사 내용입니다");
    expect(markup).not.toContain("Final");
    expect(markup).not.toContain("Transcribing");
  });

  it("applies caption style variables from the backend snapshot", () => {
    const markup = render(
      caption({
        chunk_id: 1,
        phase: "partial",
        text: "사용자가 설정한 스타일입니다",
        visible: true,
        style: {
          font_size_px: 32,
          text_color: "#f8fafc",
          background_color: "#123456",
          background_opacity: 68,
        },
      } as Partial<OverlayCaptionSnapshot>),
    );

    expect(markup).toContain("--overlay-caption-font-size:32px");
    expect(markup).toContain("--overlay-caption-text-color:#f8fafc");
    expect(markup).toContain("--overlay-caption-background-rgb:18 52 86");
    expect(markup).toContain("--overlay-caption-background-alpha:0.68");
  });

  it("prioritizes recording dictation over a visible caption", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={caption({
          visible: true,
          text: "caption",
        })}
        dictation={{ state: "recording", message: null }}
        levels={[0.1, 0.4, 0.8]}
        elapsedSeconds={4}
      />,
    );

    expect(markup).toContain('data-dictation-state="recording"');
    expect(markup).toContain("0:04");
    expect(markup).toContain("overlay-dictation__waveform");
    expect(markup).not.toContain("caption");
  });

  it("keeps transcribing visible without waveform bars", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={emptyCaptionSnapshot()}
        dictation={{ state: "transcribing", message: "Transcribing…" }}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain('data-dictation-state="transcribing"');
    expect(markup).toContain("Transcribing…");
    expect(markup).toContain("overlay-dictation__spinner");
    expect(markup).not.toContain("overlay-dictation__waveform");
  });

  it("renders an actionable error without a spinner", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={emptyCaptionSnapshot()}
        dictation={{ state: "error", message: "Dictation failed" }}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain('data-dictation-state="error"');
    expect(markup).toContain("Dictation failed");
    expect(markup).toContain("overlay-dictation__dot");
    expect(markup).not.toContain("overlay-dictation__spinner");
  });
});
