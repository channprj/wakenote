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
      onResetThreshold={() => {}}
    />,
  );
}

function buttonTag(markup: string, label: string) {
  const match = markup.match(
    new RegExp(`<button[^>]*>(?:(?!</button>)[\\s\\S])*?${label}(?:(?!</button>)[\\s\\S])*?</button>`),
  );
  expect(match, `expected ${label} button`).not.toBeNull();
  return match?.[0] ?? "";
}

function hasButton(markup: string, label: string) {
  return new RegExp(
    `<button[^>]*>(?:(?!</button>)[\\s\\S])*?${label}(?:(?!</button>)[\\s\\S])*?</button>`,
  ).test(markup);
}

function isDisabled(button: string) {
  return /\sdisabled(=""|\s|>)/.test(button);
}

describe("level meter", () => {
  it("only shows live levels and calibration actions while live input is active", () => {
    const inactive = renderLevelMeter(false);

    expect(inactive).toContain("-120 dBFS");
    expect(inactive).toContain("-90 dBFS");
    expect(inactive).not.toContain("-18 dBFS");
    expect(inactive).not.toContain("-55 dBFS");
    expect(inactive).not.toContain("-43 dBFS");
    expect(isDisabled(buttonTag(inactive, "Calibrate"))).toBe(true);
    expect(isDisabled(buttonTag(inactive, "Reset"))).toBe(false);
    expect(hasButton(inactive, "Apply")).toBe(false);

    const active = renderLevelMeter(true);

    expect(active).toContain("-22 dBFS");
    expect(isDisabled(buttonTag(active, "Calibrate"))).toBe(false);
    expect(isDisabled(buttonTag(active, "Reset"))).toBe(false);
    expect(hasButton(active, "Apply")).toBe(false);
  });

  it("title-cases the tray_state Badge text to match QueuePanel's status badges", () => {
    const inactive = renderLevelMeter(false);
    expect(inactive).toMatch(/<span class="ui-badge ui-badge--neutral">Idle<\/span>/);
    expect(inactive).not.toMatch(/<span class="ui-badge[^"]*">idle<\/span>/);

    const active = renderLevelMeter(true);
    expect(active).toMatch(/<span class="ui-badge ui-badge--success">Listening<\/span>/);
    expect(active).not.toMatch(/<span class="ui-badge[^"]*">listening<\/span>/);
  });
});
