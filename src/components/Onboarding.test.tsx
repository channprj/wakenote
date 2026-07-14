import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { defaultSettings, mockModels } from "@/lib/app-state";
import { Onboarding } from "./Onboarding";

function readyModels() {
  const settings = defaultSettings();
  return mockModels().map((model) =>
    model.id === settings.selected_model
      ? { ...model, status: "ready" as const }
      : model,
  );
}

describe("Onboarding", () => {
  it("renders nothing after every required setup blocker is resolved", () => {
    const markup = renderToStaticMarkup(
      <Onboarding
        settings={{ ...defaultSettings(), save_root_confirmed: true }}
        models={readyModels()}
        microphones={[
          { id: "default", label: "System Default", available: true, fallback: true },
        ]}
      />,
    );

    expect(markup).toBe("");
  });

  it("shows only incomplete microphone, storage, and model blockers", () => {
    const markup = renderToStaticMarkup(
      <Onboarding
        settings={{
          ...defaultSettings(),
          save_root_confirmed: false,
          calibration_completed: false,
        }}
        models={mockModels()}
        microphones={[]}
      />,
    );

    expect(markup).toContain("Microphone");
    expect(markup).toContain("Save Root");
    expect(markup).toContain("Model");
    expect(markup).not.toContain("Calibration");
    expect(markup).not.toContain('data-complete="true"');
  });
});
