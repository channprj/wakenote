import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { RecorderWaveform } from "./RecorderWaveform";

describe("RecorderWaveform", () => {
  it("renders real RMS history as a centered accessible SVG", () => {
    const markup = renderToStaticMarkup(
      <RecorderWaveform
        levels={[0, 0.25, 0.75, 1]}
        width={320}
        currentDbfs={-22}
        peakDbfs={-8}
        thresholdDbfs={-40}
        state="recording"
      />,
    );

    expect(markup).toContain('role="img"');
    expect(markup).toContain("Current -22 dBFS");
    expect(markup).toContain("Peak -8 dBFS");
    expect(markup).toContain("Threshold -40 dBFS");
    expect(markup).toContain('data-state="recording"');
    expect(markup).toContain('data-bar-count="48"');
    expect(markup).toContain('data-level="1"');
    expect(markup).toContain('y="6"');
    expect(markup).toContain('height="88"');
    expect(markup).not.toContain("<canvas");
  });

  it("uses a flat, non-random baseline when no level history exists", () => {
    const markup = renderToStaticMarkup(
      <RecorderWaveform
        levels={[]}
        width={720}
        currentDbfs={-120}
        peakDbfs={-120}
        thresholdDbfs={-40}
        state="idle"
      />,
    );

    expect(markup).toContain('data-bar-count="96"');
    expect(markup.match(/data-level="0"/g)).toHaveLength(96);
    expect(markup).not.toMatch(/data-level="0\.[1-9]/);
  });
});
