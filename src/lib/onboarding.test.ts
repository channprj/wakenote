import { describe, expect, it } from "vitest";
import { defaultSettings, mockModels } from "./app-state";
import { deriveOnboardingSteps } from "./onboarding";

describe("onboarding setup steps", () => {
  it("requires an available selected microphone or usable fallback before microphone setup is complete", () => {
    const settings = defaultSettings();
    const models = mockModels();

    expect(
      deriveOnboardingSteps(settings, models, [
        { id: "default", label: "System Default", available: false, fallback: false },
      ]).find((step) => step.id === "microphone")?.complete,
    ).toBe(false);

    expect(
      deriveOnboardingSteps(
        {
          ...settings,
          selected_microphone: "input-missing-airpods",
          selected_microphone_label: "Missing AirPods",
        },
        models,
        [
          { id: "default", label: "System Default", available: true, fallback: true },
          { id: "input-missing-airpods", label: "Missing AirPods", available: false, fallback: false },
        ],
      ).find((step) => step.id === "microphone")?.complete,
    ).toBe(true);
  });

  it("requires the selected model to be installed before model setup is complete", () => {
    const settings = defaultSettings();
    const missingModels = mockModels();

    expect(deriveOnboardingSteps(settings, missingModels).find((step) => step.id === "model")?.complete).toBe(false);

    const readyModels = missingModels.map((model) =>
      model.id === settings.selected_model ? { ...model, status: "ready" as const } : model,
    );

    expect(deriveOnboardingSteps(settings, readyModels).find((step) => step.id === "model")?.complete).toBe(true);
  });

  it("requires explicit threshold calibration before calibration setup is complete", () => {
    const models = mockModels();
    const microphones = [{ id: "default", label: "System Default", available: true, fallback: false }];
    const uncalibratedSettings = {
      ...defaultSettings(),
      calibration_completed: false,
    };
    const calibratedSettings = {
      ...defaultSettings(),
      calibration_completed: true,
    };

    expect(
      deriveOnboardingSteps(uncalibratedSettings, models, microphones).find(
        (step) => step.id === "calibration",
      )?.complete,
    ).toBe(false);
    expect(
      deriveOnboardingSteps(calibratedSettings, models, microphones).find(
        (step) => step.id === "calibration",
      )?.complete,
    ).toBe(true);
  });
});
