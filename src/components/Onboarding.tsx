import { Brain, CheckCircle2, Folder, Gauge, Mic } from "lucide-react";
import { deriveOnboardingSteps, type OnboardingStepId } from "../lib/onboarding";
import type { AppSettings, MicrophoneDevice, ModelDescriptor } from "../lib/types";

const stepIcons: Record<OnboardingStepId, typeof Mic> = {
  microphone: Mic,
  save_root: Folder,
  model: Brain,
  calibration: Gauge,
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
  const steps = deriveOnboardingSteps(settings, models, microphones);

  return (
    <div className="onboarding-strip">
      {steps.map((step) => {
        const Icon = stepIcons[step.id];
        return (
          <div key={step.id} data-complete={step.complete}>
            <Icon />
            <span>{step.label}</span>
            <CheckCircle2 />
          </div>
        );
      })}
    </div>
  );
}
