import type { AppSettings, ModelDescriptor } from "./types";

export type OnboardingStepId = "microphone" | "save_root" | "model" | "calibration";

export interface OnboardingStep {
  id: OnboardingStepId;
  label: string;
  complete: boolean;
}

function modelIsAvailable(model: ModelDescriptor | undefined) {
  return Boolean(model && ["installed", "ready", "unloaded"].includes(model.status));
}

export function deriveOnboardingSteps(
  settings: AppSettings,
  models: ModelDescriptor[],
): OnboardingStep[] {
  const selectedModel = models.find((model) => model.id === settings.selected_model);

  return [
    { id: "microphone", label: "Microphone", complete: Boolean(settings.selected_microphone) },
    { id: "save_root", label: "Save Root", complete: Boolean(settings.save_root) },
    { id: "model", label: "Model", complete: modelIsAvailable(selectedModel) },
    { id: "calibration", label: "Calibration", complete: settings.threshold_dbfs < 0 },
  ];
}
