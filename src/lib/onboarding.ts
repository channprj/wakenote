import type { AppSettings, MicrophoneDevice, ModelDescriptor } from "./types";

export type OnboardingStepId = "microphone" | "save_root" | "model" | "calibration";

export interface OnboardingStep {
  id: OnboardingStepId;
  label: string;
  complete: boolean;
}

function modelIsAvailable(model: ModelDescriptor | undefined) {
  return Boolean(model && ["installed", "ready", "unloaded"].includes(model.status));
}

function microphoneCanStart(settings: AppSettings, microphones: MicrophoneDevice[]) {
  const selectedMicrophone = microphones.find((mic) => mic.id === settings.selected_microphone);
  const fallbackMicrophone = microphones.find((mic) => mic.fallback && mic.available);

  return Boolean(selectedMicrophone?.available || fallbackMicrophone);
}

export function deriveOnboardingSteps(
  settings: AppSettings,
  models: ModelDescriptor[],
  microphones: MicrophoneDevice[] = [],
): OnboardingStep[] {
  const selectedModel = models.find((model) => model.id === settings.selected_model);

  return [
    { id: "microphone", label: "Microphone", complete: microphoneCanStart(settings, microphones) },
    { id: "save_root", label: "Save Root", complete: Boolean(settings.save_root) },
    { id: "model", label: "Model", complete: modelIsAvailable(selectedModel) },
    { id: "calibration", label: "Calibration", complete: settings.calibration_completed },
  ];
}
