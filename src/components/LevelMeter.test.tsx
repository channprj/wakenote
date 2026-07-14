import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "../lib/app-state";
import type { TrayState } from "../lib/types";
import { LevelMeter, calibrateDisabledReason } from "./LevelMeter";

function renderLevelMeter(liveInputActive: boolean, trayState?: TrayState) {
  const snapshot = mockSnapshot();

  return renderToStaticMarkup(
    <LevelMeter
      settings={snapshot.settings}
      status={{
        ...snapshot.status,
        live_input_active: liveInputActive,
        tray_state: trayState ?? (liveInputActive ? "listening" : "idle"),
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

function titleOf(button: string): string | null {
  const match = button.match(/\stitle="([^"]*)"/);
  return match ? match[1] : null;
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
    expect(inactive).toMatch(/<span [^>]*data-tone="neutral">Idle<\/span>/);
    expect(inactive).not.toMatch(/<span [^>]*data-tone="neutral">idle<\/span>/);

    const active = renderLevelMeter(true);
    expect(active).toMatch(/<span [^>]*data-tone="primary">Listening<\/span>/);
    expect(active).not.toMatch(/<span [^>]*data-tone="primary">listening<\/span>/);
  });

  describe.each<[TrayState, string, string]>([
    ["idle", "neutral", "Idle"],
    ["listening", "primary", "Listening"],
    ["recording", "success", "Recording"],
    ["transcribing", "warning", "Transcribing"],
    ["paused", "warning", "Paused"],
    ["error", "danger", "Error"],
  ])(
    "tones the tray_state Badge via trayStateBadgeTone for every tray_state value",
    (trayState, expectedTone, expectedText) => {
      it(`renders data-tone=${expectedTone} for tray_state=${trayState}`, () => {
        const markup = renderLevelMeter(trayState !== "idle", trayState);
        expect(markup).toMatch(
          new RegExp(`<span [^>]*data-tone="${expectedTone}">${expectedText}</span>`),
        );
      });
    },
  );

  it("surfaces a 'why disabled' title on the Calibrate button when live input is inactive", () => {
    const inactive = renderLevelMeter(false);
    expect(titleOf(buttonTag(inactive, "Calibrate"))).toBe("Start input to calibrate");

    const active = renderLevelMeter(true);
    expect(titleOf(buttonTag(active, "Calibrate"))).toBeNull();
  });
});

describe("calibrateDisabledReason", () => {
  it("returns a reason when live input is inactive", () => {
    expect(calibrateDisabledReason({ live_input_active: false })).toBe(
      "Start input to calibrate",
    );
  });

  it("returns null when live input is active", () => {
    expect(calibrateDisabledReason({ live_input_active: true })).toBeNull();
  });
});
