import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import { QuickCalibration } from "./QuickCalibration";

describe("QuickCalibration", () => {
  it("shows compact live level guidance and disabled calibration reason", () => {
    const snapshot = mockSnapshot();
    const markup = renderToStaticMarkup(
      <QuickCalibration
        settings={snapshot.settings}
        status={{
          ...snapshot.status,
          live_input_active: false,
          level: {
            current_dbfs: -22,
            peak_dbfs: -18,
            noise_floor_dbfs: -55,
            suggested_threshold_dbfs: -43,
          },
        }}
        onApplyThreshold={() => {}}
        onResetRecordingSettings={() => {}}
      />,
    );

    expect(markup).toContain("Noise floor");
    expect(markup).toContain("Suggested");
    expect(markup).toContain("Threshold");
    expect(markup).toContain('title="Start input to calibrate"');
    expect(markup).not.toContain("-55 dBFS");
  });
});
