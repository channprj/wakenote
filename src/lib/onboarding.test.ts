import { describe, expect, it } from "vitest";
import { defaultSettings, mockModels } from "./app-state";
import { deriveOnboardingSteps } from "./onboarding";

describe("onboarding setup steps", () => {
  it("requires the selected model to be installed before model setup is complete", () => {
    const settings = defaultSettings();
    const missingModels = mockModels();

    expect(deriveOnboardingSteps(settings, missingModels).find((step) => step.id === "model")?.complete).toBe(false);

    const readyModels = missingModels.map((model) =>
      model.id === settings.selected_model ? { ...model, status: "ready" as const } : model,
    );

    expect(deriveOnboardingSteps(settings, readyModels).find((step) => step.id === "model")?.complete).toBe(true);
  });
});
