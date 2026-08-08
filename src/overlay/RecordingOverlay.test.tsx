import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  OverlayContent,
  emptyCaptionSnapshot,
  type DictationOverlaySnapshot,
  type DictationOverlayStyle,
  type OverlayCaptionSnapshot,
} from "./RecordingOverlay";

const hiddenDictation: DictationOverlaySnapshot = {
  state: "hidden",
  message: null,
  style: {
    background_color: "#000000",
    background_opacity: 88,
  },
};

function dictation(
  state: DictationOverlaySnapshot["state"],
  message: string | null,
  style: Partial<DictationOverlayStyle> = {},
): DictationOverlaySnapshot {
  return {
    state,
    message,
    style: {
      background_color: "#000000",
      background_opacity: 88,
      ...style,
    },
  };
}

function caption(
  patch: Partial<OverlayCaptionSnapshot>,
): OverlayCaptionSnapshot {
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

  it("identifies the caption source in overlay markup", () => {
    const markup = render(
      caption({
        source: "dictation",
        chunk_id: 1,
        phase: "partial",
        text: "딕테이션 중간 자막",
        visible: true,
      }),
    );

    expect(markup).toContain('data-source="dictation"');
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
          background_color: "#ffffff",
          background_opacity: 68,
          animation: "dissolve",
          duration_seconds: 7,
          padding_horizontal_px: 22,
          padding_vertical_px: 11,
          border_width_px: 3,
          border_color: "#12abef",
          border_radius_px: 16,
          max_width_px: 840,
          min_height_px: 64,
          max_height_px: 640,
        },
      } as Partial<OverlayCaptionSnapshot>),
    );

    expect(markup).toContain("--overlay-caption-font-size:32px");
    expect(markup).toContain("--overlay-caption-text-color:#f8fafc");
    expect(markup).toContain("--overlay-caption-background-rgb:255 255 255");
    expect(markup).toContain("--overlay-caption-background-alpha:0.68");
    expect(markup).toContain("--overlay-caption-padding-horizontal:22px");
    expect(markup).toContain("--overlay-caption-padding-vertical:11px");
    expect(markup).toContain("--overlay-caption-border-width:3px");
    expect(markup).toContain("--overlay-caption-border-rgb:18 171 239");
    expect(markup).toContain("--overlay-caption-border-radius:16px");
    expect(markup).toContain("--overlay-caption-max-width:840px");
    expect(markup).toContain("--overlay-caption-min-height:64px");
    expect(markup).toContain("--overlay-caption-max-height:640px");
    expect(markup).toContain("background-color:rgba(255, 255, 255, 0.68)");
    expect(markup).toContain('data-animation="dissolve"');
  });

  it("allows 10px subtitles and marks animated exit state", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={caption({
          chunk_id: 1,
          phase: "final",
          text: "작은 자막",
          visible: true,
          style: {
            ...emptyCaptionSnapshot().style,
            font_size_px: 4,
            animation: "fade",
          },
        })}
        dictation={hiddenDictation}
        levels={[]}
        elapsedSeconds={0}
        captionExiting
      />,
    );

    expect(markup).toContain("--overlay-caption-font-size:10px");
    expect(markup).toContain('data-animation="fade"');
    expect(markup).toContain('data-visibility="exiting"');
  });

  it("prioritizes recording dictation over a visible Dictation caption", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={caption({
          source: "dictation",
          visible: true,
          text: "caption",
        })}
        dictation={dictation("recording", null)}
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
        dictation={dictation("transcribing", "Transcribing…")}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain('data-dictation-state="transcribing"');
    expect(markup).toContain("Transcribing…");
    expect(markup).toContain("overlay-dictation__spinner");
    expect(markup).not.toContain("overlay-dictation__waveform");
  });

  it("keeps transcribing above a visible Live transcription caption", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={caption({
          source: "live_transcription",
          visible: true,
          text: "라이브 자막",
        })}
        dictation={dictation("transcribing", "Transcribing…")}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain("Transcribing…");
    expect(markup).toContain("overlay-dictation__spinner");
    expect(markup).not.toContain("라이브 자막");
  });

  it("replaces transcribing with a visible Dictation caption", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={caption({
          source: "dictation",
          phase: "partial",
          visible: true,
          text: "딕테이션 자막",
        })}
        dictation={dictation("transcribing", "Transcribing…")}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain("딕테이션 자막");
    expect(markup).toContain('data-source="dictation"');
    expect(markup).not.toContain("Transcribing…");
    expect(markup).not.toContain("overlay-dictation__spinner");
  });

  it("renders an actionable error without a spinner", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={emptyCaptionSnapshot()}
        dictation={dictation("error", "Dictation failed")}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain('data-dictation-state="error"');
    expect(markup).toContain("Dictation failed");
    expect(markup).toContain("overlay-dictation__dot");
    expect(markup).not.toContain("overlay-dictation__spinner");
  });

  it("keeps Dictation errors above visible Dictation captions", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={caption({
          source: "dictation",
          visible: true,
          text: "딕테이션 자막",
        })}
        dictation={dictation("error", "Dictation failed")}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain("Dictation failed");
    expect(markup).not.toContain("딕테이션 자막");
  });

  it("derives contrasting Dictation variables at both opacity boundaries", () => {
    const whiteMarkup = renderToStaticMarkup(
      <OverlayContent
        caption={emptyCaptionSnapshot()}
        dictation={dictation("transcribing", null, {
          background_color: "#ffffff",
          background_opacity: 0,
        })}
        levels={[]}
        elapsedSeconds={0}
      />,
    );
    expect(whiteMarkup).toContain("--dictation-background-rgb:255 255 255");
    expect(whiteMarkup).toContain("--dictation-background-opacity:0");
    expect(whiteMarkup).toContain("--dictation-foreground-rgb:0 0 0");
    expect(whiteMarkup).toContain("background-color:rgba(255, 255, 255, 0)");

    const blackMarkup = renderToStaticMarkup(
      <OverlayContent
        caption={emptyCaptionSnapshot()}
        dictation={dictation("error", null, {
          background_color: "#000000",
          background_opacity: 100,
        })}
        levels={[]}
        elapsedSeconds={0}
      />,
    );
    expect(blackMarkup).toContain("--dictation-background-rgb:0 0 0");
    expect(blackMarkup).toContain("--dictation-background-opacity:1");
    expect(blackMarkup).toContain("--dictation-foreground-rgb:255 255 255");
    expect(blackMarkup).toContain("background-color:rgba(0, 0, 0, 1)");
  });

  it("applies the requested background alpha while Dictation is transcribing", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={emptyCaptionSnapshot()}
        dictation={dictation("transcribing", "Transcribing…", {
          background_color: "#000000",
          background_opacity: 23,
        })}
        levels={[]}
        elapsedSeconds={0}
      />,
    );

    expect(markup).toContain("--dictation-background-opacity:0.23");
    expect(markup).toContain("background-color:rgba(0, 0, 0, 0.23)");
  });
});
