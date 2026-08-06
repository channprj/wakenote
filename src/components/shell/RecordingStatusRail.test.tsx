import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { RecordingStatusRail } from "./RecordingStatusRail";

function renderRail({
  liveActive,
  latestText,
  dictationState = "idle",
}: {
  liveActive: boolean;
  latestText: string;
  dictationState?: "idle" | "recording" | "transcribing";
}) {
  return renderToStaticMarkup(
    <RecordingStatusRail
      liveActive={liveActive}
      latestText={latestText}
      dictationState={dictationState}
      onCancelDictation={() => {}}
      onReturnToCapture={() => {}}
    />,
  );
}

describe("RecordingStatusRail", () => {
  it("stays out of layout when there is no recording context", () => {
    expect(renderRail({ liveActive: false, latestText: "" })).toBe("");
  });

  it("keeps long transcript text truncated with full-value access", () => {
    const longText = "공백 없는 매우 긴 전사".repeat(20);
    const markup = renderRail({ liveActive: true, latestText: longText });

    expect(markup).toContain("Return to Capture");
    expect(markup).toContain(`title="${longText}"`);
    expect(markup).toContain("min-w-0 truncate");
    expect(markup).toContain("shrink-0");
  });

  it("offers cancellation while Dictation records or transcribes", () => {
    expect(
      renderRail({
        liveActive: false,
        latestText: "",
        dictationState: "recording",
      }),
    ).toContain("Cancel Dictation");
    expect(
      renderRail({
        liveActive: false,
        latestText: "",
        dictationState: "transcribing",
      }),
    ).toContain("Dictation transcribing");
  });
});
