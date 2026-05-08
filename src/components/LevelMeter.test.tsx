import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "../lib/app-state";
import { LevelMeter } from "./LevelMeter";

function renderLevelMeter(liveInputActive: boolean) {
  const snapshot = mockSnapshot();

  return renderToStaticMarkup(
    <LevelMeter
      settings={snapshot.settings}
      status={{
        ...snapshot.status,
        live_input_active: liveInputActive,
        tray_state: liveInputActive ? "listening" : "idle",
        level: {
          current_dbfs: -22,
          peak_dbfs: -18,
          noise_floor_dbfs: -55,
          suggested_threshold_dbfs: -43,
        },
      }}
      onApplyThreshold={() => {}}
    />,
  );
}

function buttonTag(markup: string, label: string) {
  const match = markup.match(new RegExp(`<button[^>]*>[\\s\\S]*?${label}[\\s\\S]*?</button>`));
  expect(match, `expected ${label} button`).not.toBeNull();
  return match?.[0] ?? "";
}

function isDisabled(button: string) {
  return /\sdisabled(=""|\s|>)/.test(button);
}

describe("level meter", () => {
  it("only shows live levels and calibration actions while live input is active", () => {
    const inactive = renderLevelMeter(false);

    expect(inactive).toContain("-120 dBFS");
    expect(isDisabled(buttonTag(inactive, "Calibrate"))).toBe(true);
    expect(isDisabled(buttonTag(inactive, "Apply"))).toBe(true);

    const active = renderLevelMeter(true);

    expect(active).toContain("-22 dBFS");
    expect(isDisabled(buttonTag(active, "Calibrate"))).toBe(false);
    expect(isDisabled(buttonTag(active, "Apply"))).toBe(false);
  });
});
