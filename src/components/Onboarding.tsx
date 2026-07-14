import { Brain, CircleAlertIcon, Folder, Mic } from "lucide-react";
import { deriveOnboardingSteps, type OnboardingStepId } from "../lib/onboarding";
import type { AppSettings, MicrophoneDevice, ModelDescriptor } from "../lib/types";

const stepIcons: Record<OnboardingStepId, typeof Mic> = {
  microphone: Mic,
  save_root: Folder,
  model: Brain,
  calibration: CircleAlertIcon,
};

export function Onboarding({
  settings,
  models,
  microphones,
}: {
  settings: AppSettings;
  models: ModelDescriptor[];
  microphones: MicrophoneDevice[];
}) {
  const blockers = deriveOnboardingSteps(settings, models, microphones).filter(
    (step) => step.id !== "calibration" && !step.complete,
  );

  if (blockers.length === 0) {
    return null;
  }

  return (
    <div className="onboarding-strip">
      {blockers.map((step) => {
        const Icon = stepIcons[step.id];
        return (
          <div key={step.id} data-complete="false">
            <Icon />
            <span>{step.label}</span>
            <CircleAlertIcon />
          </div>
        );
      })}
    </div>
  );
}
