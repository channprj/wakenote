// @vitest-environment jsdom
import { renderToStaticMarkup } from "react-dom/server";
import { render as renderDom } from "@testing-library/react";
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

  it("renders final caption text verbatim without status labels", () => {
    // Balanced line breaks from the caption runtime are preserved as-is.
    const balancedText = "균형 잡힌 첫 줄\n읽기 편한 두 번째 줄";
    const markup = render(
      caption({
        chunk_id: 1,
        phase: "final",
        text: balancedText,
        visible: true,
      }),
    );

    expect(markup).toContain(balancedText);
    expect(markup).not.toContain("Final");
    expect(markup).not.toContain("Transcribing");
  });

  it("remounts only when a caption page generation changes", () => {
    const { container, rerender } = renderDom(
      <OverlayContent
        caption={caption({ generation: 4, visible: true, text: "first" })}
        dictation={hiddenDictation}
        levels={[]}
        elapsedSeconds={0}
      />,
    );
    const initial = container.querySelector(".overlay-caption");

    rerender(
      <OverlayContent
        caption={caption({
          generation: 4,
          visible: true,
          text: "first revised",
        })}
        dictation={hiddenDictation}
        levels={[]}
        elapsedSeconds={0}
      />,
    );
    expect(container.querySelector(".overlay-caption")).toBe(initial);

    rerender(
      <OverlayContent
        caption={caption({ generation: 5, visible: true, text: "new page" })}
        dictation={hiddenDictation}
        levels={[]}
        elapsedSeconds={0}
      />,
    );
    expect(container.querySelector(".overlay-caption")).not.toBe(initial);
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
    expect(markup).toContain("--overlay-caption-text-color:#000000");
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
    expect(markup).toContain(
      "--overlay-caption-outline:rgba(255, 255, 255, 0.68)",
    );
    expect(markup).toContain("--overlay-caption-outline-width:4px");
    expect(markup).not.toContain("background-color:");
    expect(markup).toContain('data-animation="dissolve"');
  });

  it("strokes unequal caption lines without a filled plate", () => {
    const markup = render(
      caption({
        chunk_id: 1,
        phase: "final",
        text: "짧은 줄\n훨씬 더 긴 자막 줄입니다",
        visible: true,
      }),
    );

    expect(markup).toContain("--overlay-caption-outline:rgba(0, 0, 0, 0.82)");
    expect(markup).toContain("--overlay-caption-outline-width:3px");
    expect(markup).toContain("--overlay-caption-text-color:#ffffff");
    expect(markup).not.toContain("background-color:");

    const mismatchedFill = render(
      caption({
        chunk_id: 3,
        phase: "final",
        text: "색이 어긋난 자막",
        visible: true,
        style: {
          ...emptyCaptionSnapshot().style,
          background_color: "#000000",
          text_color: "#f8fafc",
        },
      }),
    );
    expect(mismatchedFill).toContain("--overlay-caption-text-color:#ffffff");
    expect(mismatchedFill).toContain("--overlay-caption-outline:rgba(0, 0, 0, 0.82)");
    expect(mismatchedFill).not.toContain("background-color:");
    expect(markup.match(/overlay-caption__text/g)).toHaveLength(1);
    expect(markup).toContain("짧은 줄\n훨씬 더 긴 자막 줄입니다");

    const white = render(
      caption({
        chunk_id: 2,
        phase: "final",
        text: "흰 윤곽",
        visible: true,
        style: {
          ...emptyCaptionSnapshot().style,
          background_color: "#ffffff",
          background_opacity: 100,
          text_color: "#000000",
          font_size_px: 48,
        },
      }),
    );
    expect(white).toContain("--overlay-caption-outline:rgba(255, 255, 255, 1)");
    expect(white).toContain("--overlay-caption-outline-width:6px");
    expect(white).not.toContain("background-color:");
  });

  it("allows 8px subtitles and marks animated exit state", () => {
    const markup = renderToStaticMarkup(
      <OverlayContent
        caption={caption({
          chunk_id: 1,
          phase: "final",
          text: "작은 자막",
          visible: true,
          style: {
            ...emptyCaptionSnapshot().style,
            font_size_px: 8,
            animation: "fade",
          },
        })}
        dictation={hiddenDictation}
        levels={[]}
        elapsedSeconds={0}
        captionExiting
      />,
    );

    expect(markup).toContain("--overlay-caption-font-size:8px");
    expect(markup).toContain("--overlay-caption-outline-width:1px");
    const clamped = render(
      caption({
        chunk_id: 1,
        phase: "final",
        text: "더 작은 자막",
        visible: true,
        style: {
          ...emptyCaptionSnapshot().style,
          font_size_px: 4,
        },
      } as Partial<OverlayCaptionSnapshot>),
    );
    expect(clamped).toContain("--overlay-caption-font-size:8px");
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

  it("keeps transcribing visible above non-dictation captions without waveform bars", () => {
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

    expect(markup).toContain('data-dictation-state="transcribing"');
    expect(markup).toContain("Transcribing…");
    expect(markup).toContain("overlay-dictation__spinner");
    expect(markup).not.toContain("overlay-dictation__waveform");
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

  it("renders an actionable Dictation error above captions without a spinner", () => {
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

    expect(markup).toContain('data-dictation-state="error"');
    expect(markup).toContain("Dictation failed");
    expect(markup).toContain("overlay-dictation__dot");
    expect(markup).not.toContain("overlay-dictation__spinner");
    expect(markup).not.toContain("딕테이션 자막");
  });

  it("derives Dictation background variables across opacity values", () => {
    const cases: {
      label: string;
      state: DictationOverlaySnapshot["state"];
      message: string | null;
      style: Partial<DictationOverlayStyle>;
      rgb: string;
      opacity: string;
      foreground?: string;
      backgroundColor: string;
    }[] = [
      {
        label: "white at the 0 boundary",
        state: "transcribing",
        message: null,
        style: { background_color: "#ffffff", background_opacity: 0 },
        rgb: "255 255 255",
        opacity: "0",
        foreground: "0 0 0",
        backgroundColor: "rgba(255, 255, 255, 0)",
      },
      {
        label: "black at the 100 boundary",
        state: "error",
        message: null,
        style: { background_color: "#000000", background_opacity: 100 },
        rgb: "0 0 0",
        opacity: "1",
        foreground: "255 255 255",
        backgroundColor: "rgba(0, 0, 0, 1)",
      },
      {
        label: "requested alpha while transcribing",
        state: "transcribing",
        message: "Transcribing…",
        style: { background_color: "#000000", background_opacity: 23 },
        rgb: "0 0 0",
        opacity: "0.23",
        backgroundColor: "rgba(0, 0, 0, 0.23)",
      },
    ];

    for (const row of cases) {
      const markup = renderToStaticMarkup(
        <OverlayContent
          caption={emptyCaptionSnapshot()}
          dictation={dictation(row.state, row.message, row.style)}
          levels={[]}
          elapsedSeconds={0}
        />,
      );

      expect(markup, row.label).toContain(
        `--dictation-background-rgb:${row.rgb}`,
      );
      expect(markup, row.label).toContain(
        `--dictation-background-opacity:${row.opacity}`,
      );
      if (row.foreground) {
        expect(markup, row.label).toContain(
          `--dictation-foreground-rgb:${row.foreground}`,
        );
      }
      expect(markup, row.label).toContain(
        `background-color:${row.backgroundColor}`,
      );
    }
  });
});
