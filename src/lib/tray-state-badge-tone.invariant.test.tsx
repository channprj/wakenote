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
// from the shared trayStateBadgeTone() helper. Every surface uses the shadcn StatusBadge data-tone
// contract, so this suite checks semantic agreement across all renderers.

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
      onResetRecordingSettings={() => {}}
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

function extractTrayStateBadgeTone(markup: string, humanizedText: string): string {
  const match = markup.match(
    new RegExp(`<span[^>]*data-tone="([a-z]+)"[^>]*>${humanizedText}</span>`),
  );
  expect(match, `expected tray_state Badge with text "${humanizedText}"`).not.toBeNull();
  return match?.[1] ?? "";
}

function extractAppCaptureBadgeTone(markup: string, humanizedText: string): string {
  const match = markup.match(
    new RegExp(`<span[^>]*data-tone="([a-z]+)"[^>]*>${humanizedText}</span>`),
  );
  expect(match, `expected Capture StatusBadge with text "${humanizedText}"`).not.toBeNull();
  return match?.[1] ?? "";
}

describe.each<TrayState>(["idle", "listening", "recording", "transcribing", "paused", "error"])(
  "tray_state Badge cross-surface invariant for %s",
  (trayState) => {
    it("renders the same semantic tone in Capture, TrayPreview, and LevelMeter", () => {
      const snapshot = buildSnapshotWithTrayState(trayState);
      const humanized = humanizeTrayState(trayState);
      const expectedTone = trayStateBadgeTone(trayState);
      const appTone = extractAppCaptureBadgeTone(renderAppMarkup(snapshot), humanized);
      const trayPreviewTone = extractTrayStateBadgeTone(
        renderTrayPreviewBadge(snapshot),
        humanized,
      );
      const levelMeterTone = extractTrayStateBadgeTone(
        renderLevelMeterBadge(snapshot),
        humanized,
      );

      expect(appTone).toBe(expectedTone);
      expect(trayPreviewTone).toBe(expectedTone);
      expect(levelMeterTone).toBe(expectedTone);
      expect(trayPreviewTone).toBe(levelMeterTone);
    });
  },
);
