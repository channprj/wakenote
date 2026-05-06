import { Brain, CheckCircle2, Folder, Gauge, Mic } from "lucide-react";
import type { AppSettings, ModelDescriptor } from "../lib/types";

export function Onboarding({
  settings,
  models,
}: {
  settings: AppSettings;
  models: ModelDescriptor[];
}) {
  const selectedModel = models.find((model) => model.id === settings.selected_model);
  const steps = [
    { icon: Mic, label: "Microphone", complete: Boolean(settings.selected_microphone) },
    { icon: Folder, label: "Save Root", complete: Boolean(settings.save_root) },
    { icon: Brain, label: "Model", complete: Boolean(selectedModel) },
    { icon: Gauge, label: "Calibration", complete: settings.threshold_dbfs < 0 },
  ];

  return (
    <div className="onboarding-strip">
      {steps.map((step) => {
        const Icon = step.icon;
        return (
          <div key={step.label} data-complete={step.complete}>
            <Icon />
            <span>{step.label}</span>
            <CheckCircle2 />
          </div>
        );
      })}
    </div>
  );
}
