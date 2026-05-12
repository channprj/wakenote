import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

// Mock app-state BEFORE importing App so App's useState(mockSnapshot()) initial value can be
// injected per-test via vi.mocked(mockSnapshot).mockReturnValueOnce(...). The mock's default
// impl (vi.fn(actual.mockSnapshot)) delegates to the real factory when no Once override is
// active, so this file's existing TrayPreview / LevelMeter helpers — which call mockSnapshot()
// to build their snapshot prop — continue to receive the real default snapshot.
vi.mock("./app-state", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./app-state")>();
  return {
    ...actual,
    mockSnapshot: vi.fn(actual.mockSnapshot),
  };
});

import App from "../App";
import { LevelMeter } from "../components/LevelMeter";
import { TrayPreview } from "../components/TrayPreview";
import { mockSnapshot } from "./app-state";
import { trayStateBadgeTone } from "./status-summary";
import { humanizeTrayState } from "./transcript-history";
import type { AppSnapshot, TrayState } from "./types";

// Cross-surface invariant: every component that renders a tray_state Badge must derive its tone
// from the shared trayStateBadgeTone() helper. Per-component tests pin each surface individually,
// but this suite makes the cross-surface agreement EXPLICIT across all THREE consumers
// (App.tsx status-strip second Badge, TrayPreview Badge, LevelMeter State Badge) so a future
// regression where one surface reverts to a hardcoded ternary (cf. iter-70's LevelMeter
// `active ? "success" : "neutral"` bug) fails an additional dedicated test rather than just
// looking like an isolated per-component failure.

function buildSnapshotWithTrayState(trayState: TrayState): AppSnapshot {
  // Calling the mocked mockSnapshot() with no active Once override falls through to its default
  // impl (actual.mockSnapshot), so we get a real, fresh snapshot to mutate.
  const base = mockSnapshot();
  return { ...base, status: { ...base.status, tray_state: trayState } };
}

function renderTrayPreviewBadge(snapshot: AppSnapshot): string {
  const element = TrayPreview({
    settings: snapshot.settings,
    status: snapshot.status,
    onPatch: vi.fn(),
    onRevealSaveFolder: vi.fn(),
    onCancelCurrentOperation: vi.fn(),
  });
  return renderToStaticMarkup(element);
}

function renderLevelMeterBadge(snapshot: AppSnapshot): string {
  return renderToStaticMarkup(
    <LevelMeter
      settings={snapshot.settings}
      status={snapshot.status}
      onApplyThreshold={() => {}}
      onResetThreshold={() => {}}
    />,
  );
}

function renderAppMarkup(snapshot: AppSnapshot): string {
  // App.tsx initializes its snapshot state via useState(mockSnapshot()). SSR rendering invokes
  // mockSnapshot exactly once (useEffect doesn't run), so a single mockReturnValueOnce is
  // sufficient to inject the tray_state-overridden snapshot.
  vi.mocked(mockSnapshot).mockReturnValueOnce(snapshot);
  return renderToStaticMarkup(<App />);
}

function extractTrayStateBadgeClass(markup: string, humanizedText: string): string {
  const match = markup.match(
    new RegExp(`<span class="(ui-badge ui-badge--[a-z]+)">${humanizedText}</span>`),
  );
  expect(match, `expected tray_state Badge with text "${humanizedText}"`).not.toBeNull();
  return match?.[1] ?? "";
}

function extractAppStatusStripBadgeClass(markup: string, humanizedText: string): string {
  // Scope extraction to the status-strip div so the regex picks the tray_state Badge even if
  // the same humanized text appears elsewhere in App's full markup (defense against future
  // additions). The first Badge in the strip is the modeLabel ("Recording + transcription"
  // for the default settings), which never collides with the humanizeTrayState output for any
  // TrayState value, but anchoring on the strip is more robust than relying on that.
  const stripMatch = markup.match(/<div class="status-strip">([\s\S]*?)<\/div>/);
  expect(stripMatch, "expected status-strip div in App markup").not.toBeNull();
  return extractTrayStateBadgeClass(stripMatch?.[1] ?? "", humanizedText);
}

describe.each<TrayState>(["idle", "listening", "recording", "transcribing", "paused", "error"])(
  "tray_state Badge cross-surface invariant for %s",
  (trayState) => {
    it("renders the same ui-badge class in App, TrayPreview, and LevelMeter", () => {
      const snapshot = buildSnapshotWithTrayState(trayState);
      const humanized = humanizeTrayState(trayState);
      const expectedClass = `ui-badge ui-badge--${trayStateBadgeTone(trayState)}`;

      const appClass = extractAppStatusStripBadgeClass(renderAppMarkup(snapshot), humanized);
      const trayPreviewClass = extractTrayStateBadgeClass(
        renderTrayPreviewBadge(snapshot),
        humanized,
      );
      const levelMeterClass = extractTrayStateBadgeClass(
        renderLevelMeterBadge(snapshot),
        humanized,
      );

      expect(appClass).toBe(expectedClass);
      expect(trayPreviewClass).toBe(expectedClass);
      expect(levelMeterClass).toBe(expectedClass);
      expect(appClass).toBe(trayPreviewClass);
      expect(trayPreviewClass).toBe(levelMeterClass);
    });
  },
);
