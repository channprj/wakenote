import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "./app-state";
import { trayStateBadgeTone } from "./status-summary";
import { humanizeTrayState } from "./transcript-history";
import type { TrayState } from "./types";
import { LevelMeter } from "../components/LevelMeter";
import { TrayPreview } from "../components/TrayPreview";

// Cross-surface invariant: every component that renders a tray_state Badge must derive its tone
// from the shared trayStateBadgeTone() helper. The per-component tests already pin each surface's
// markup individually, but this suite makes the cross-surface agreement EXPLICIT so a future
// regression where one surface reverts to a hardcoded ternary (e.g. iter-70's LevelMeter
// `active ? "success" : "neutral"` bug) fails an additional dedicated test rather than just
// looking like an isolated per-component failure.

function renderTrayPreviewBadge(trayState: TrayState): string {
  const snapshot = mockSnapshot();
  const element = TrayPreview({
    settings: snapshot.settings,
    status: { ...snapshot.status, tray_state: trayState },
    onPatch: vi.fn(),
    onRevealSaveFolder: vi.fn(),
    onCancelCurrentOperation: vi.fn(),
  });
  return renderToStaticMarkup(element);
}

function renderLevelMeterBadge(trayState: TrayState): string {
  const snapshot = mockSnapshot();
  return renderToStaticMarkup(
    <LevelMeter
      settings={snapshot.settings}
      status={{ ...snapshot.status, tray_state: trayState }}
      onApplyThreshold={() => {}}
      onResetThreshold={() => {}}
    />,
  );
}

function extractTrayStateBadgeClass(markup: string, humanizedText: string): string {
  const match = markup.match(
    new RegExp(`<span class="(ui-badge ui-badge--[a-z]+)">${humanizedText}</span>`),
  );
  expect(match, `expected tray_state Badge with text "${humanizedText}"`).not.toBeNull();
  return match?.[1] ?? "";
}

describe.each<TrayState>(["idle", "listening", "recording", "transcribing", "paused", "error"])(
  "tray_state Badge cross-surface invariant for %s",
  (trayState) => {
    it("renders the same ui-badge class in TrayPreview and LevelMeter", () => {
      const humanized = humanizeTrayState(trayState);
      const expectedClass = `ui-badge ui-badge--${trayStateBadgeTone(trayState)}`;

      const trayPreviewClass = extractTrayStateBadgeClass(
        renderTrayPreviewBadge(trayState),
        humanized,
      );
      const levelMeterClass = extractTrayStateBadgeClass(
        renderLevelMeterBadge(trayState),
        humanized,
      );

      expect(trayPreviewClass).toBe(expectedClass);
      expect(levelMeterClass).toBe(expectedClass);
      expect(trayPreviewClass).toBe(levelMeterClass);
    });
  },
);
