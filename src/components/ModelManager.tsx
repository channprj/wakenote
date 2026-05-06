import { CheckCircle2, Download, RotateCw, ShieldCheck, Trash2 } from "lucide-react";
import { Badge, Button, Progress } from "./ui/primitives";
import type { AppSettings, ModelDescriptor, ModelStatus } from "../lib/types";

function statusTone(status: ModelStatus) {
  switch (status) {
    case "ready":
    case "installed":
      return "success";
    case "downloading":
    case "verifying":
    case "extracting":
      return "primary";
    case "error":
      return "danger";
    default:
      return "neutral";
  }
}

function statusProgress(status: ModelStatus) {
  switch (status) {
    case "downloading":
      return 42;
    case "verifying":
      return 72;
    case "extracting":
      return 88;
    case "ready":
    case "installed":
      return 100;
    default:
      return 0;
  }
}

export function ModelManager({
  models,
  settings,
  onPatch,
  onVerify,
  onDelete,
}: {
  models: ModelDescriptor[];
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
  onVerify: (modelId: string) => void;
  onDelete: (modelId: string) => void;
}) {
  return (
    <div className="model-list">
      {models.map((model) => {
        const selected = settings.selected_model === model.id;
        const progress = statusProgress(model.status);
        return (
          <article className="model-row" key={model.id} data-selected={selected}>
            <div className="model-row__main">
              <header>
                <div>
                  <strong>{model.display_name}</strong>
                  <span>
                    {model.engine} · {model.provider_runtime} · {model.size_mb} MB
                  </span>
                </div>
                <Badge tone={statusTone(model.status)}>{model.status}</Badge>
              </header>
              <Progress value={progress} />
              <div className="model-row__scores">
                <span>Speed {model.speed_score}/10</span>
                <span>Accuracy {model.accuracy_score}/10</span>
                <span>{model.languages.join(", ")}</span>
                <span>{model.offline ? "Offline" : "Cloud/API"}</span>
              </div>
            </div>
            <div className="model-row__actions">
              <Button
                type="button"
                variant={selected ? "secondary" : "primary"}
                size="sm"
                onClick={() => onPatch({ selected_model: model.id })}
              >
                <CheckCircle2 data-icon="inline-start" />
                {selected ? "Active" : "Switch"}
              </Button>
              <Button type="button" variant="secondary" size="icon" title="Download">
                <Download />
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="icon"
                title="Verify"
                onClick={() => onVerify(model.id)}
              >
                <ShieldCheck />
              </Button>
              <Button type="button" variant="secondary" size="icon" title="Retry">
                <RotateCw />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                title="Delete"
                onClick={() => onDelete(model.id)}
              >
                <Trash2 />
              </Button>
            </div>
          </article>
        );
      })}
    </div>
  );
}
