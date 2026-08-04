import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { mockSnapshot } from "./app-state";
import { modelStatusBadgeTone } from "./status-summary";
import { humanizeModelStatus } from "./transcript-history";
import type { ModelStatus } from "./types";
import { ModelManager } from "../components/ModelManager";
import { ModelsSettings } from "../components/settings/ModelsSettings";

// Cross-surface invariant: every component that renders a model_status Badge must derive its tone
// from the shared modelStatusBadgeTone() helper. The per-component tests already pin each surface's
// markup individually (ModelManager's per-row Badge and ModelsSettings' selected-model header
// Badge), but this suite makes the cross-surface agreement EXPLICIT so a future regression where
// one surface reverts to a hardcoded tone (e.g. a selected-model `tone="primary"`
// bug) fails an additional dedicated test rather than just looking like an isolated per-component
// failure. Mirrors the iter-71 tray-state invariant suite.

function renderModelManagerSelectedRowBadgeTone(status: ModelStatus): string {
  const snapshot = mockSnapshot();
  const models = snapshot.models.map((model) =>
    model.id === snapshot.settings.selected_model
      ? { ...model, status }
      : model,
  );
  const markup = renderToStaticMarkup(
    <ModelManager
      models={models}
      settings={snapshot.settings}
      onPatch={() => {}}
      onVerify={() => {}}
      onDownload={() => {}}
      onCancelDownload={() => {}}
      onDelete={() => {}}
    />,
  );
  // Anchor on the humanized status text emitted as the per-row Badge label. The selected
  // Whisper Medium row is the only model with this status in the test setup (siblings stay
  // at "missing"), so the humanized label uniquely identifies it for every non-"missing"
  // value. For "missing" itself, every row carries the same status — the assertion still
  // holds because every Badge resolves to the same tone via the shared helper.
  const humanized = humanizeModelStatus(status);
  const match = markup.match(
    new RegExp(`<span [^>]*data-tone="([a-z]+)">${humanized}</span>`),
  );
  expect(
    match,
    `expected ModelManager Badge with text "${humanized}"`,
  ).not.toBeNull();
  return match?.[1] ?? "";
}

function renderModelsSettingsSectionBadgeTone(status: ModelStatus): string {
  const snapshot = mockSnapshot();
  snapshot.models = snapshot.models.map((model) =>
    model.id === snapshot.settings.selected_model
      ? { ...model, status }
      : model,
  );
  const markup = renderToStaticMarkup(
    <ModelsSettings
      snapshot={snapshot}
      actions={{
        onPatch: () => {},
        onSuspendDictationShortcut: () => {},
        onResumeDictationShortcut: () => {},
        onPressedModifierShortcut: async () => null,
        onChooseSaveRoot: () => {},
        onRevealSaveFolder: () => {},
        onChooseModelDirectory: () => {},
        onOpenDictionaryFile: () => {},
        onReloadDictionaryFile: () => {},
        onRequestAccessibilityPermission: () => {},
        onRequestMicrophonePermission: () => {},
        onRequestScreenRecordingPermission: () => {},
        onVerifyModel: () => {},
        onDownloadModel: () => {},
        onCancelModelDownload: () => {},
        onDeleteModel: () => {},
        onSaveOpenRouterApiKey: () => {},
        onDeleteOpenRouterApiKey: () => {},
        onSaveOpenAiApiKey: () => {},
        onDeleteOpenAiApiKey: () => {},
        onSaveSonioxApiKey: () => {},
        onDeleteSonioxApiKey: () => {},
      }}
    />,
  );
  // The Models section header Badge text is the selected model's display_name ("Whisper Medium"
  // in the mock snapshot), independent of the status value. This is unique within the Models
  // section markup because per-row Badges show humanized status text instead.
  const match = markup.match(
    /<span [^>]*data-tone="([a-z]+)">Whisper Medium<\/span>/,
  );
  expect(
    match,
    'expected ModelsSettings selected-model Badge "Whisper Medium"',
  ).not.toBeNull();
  return match?.[1] ?? "";
}

describe.each<ModelStatus>([
  "installed",
  "missing",
  "downloading",
  "verifying",
  "extracting",
  "ready",
  "unloaded",
  "error",
])("model_status Badge cross-surface invariant for %s", (status) => {
  it("renders the same semantic tone in ModelManager and ModelsSettings", () => {
    const expectedTone = modelStatusBadgeTone(status);

    const modelManagerTone = renderModelManagerSelectedRowBadgeTone(status);
    const modelsSettingsTone = renderModelsSettingsSectionBadgeTone(status);

    expect(modelManagerTone).toBe(expectedTone);
    expect(modelsSettingsTone).toBe(expectedTone);
    expect(modelManagerTone).toBe(modelsSettingsTone);
  });
});
