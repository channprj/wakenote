import { CheckCircle2, CircleX, Download, RotateCw, ShieldCheck, Trash2 } from "lucide-react";
import { Badge, Button, Progress } from "./ui/primitives";
import { modelStatusBadgeTone } from "../lib/status-summary";
import { humanizeModelStatus } from "../lib/transcript-history";
import type { AppSettings, ModelDescriptor, ModelStatus } from "../lib/types";

const LANGUAGE_LABELS: Record<string, string> = {
  ko: "Korean",
  en: "English",
  ja: "Japanese",
  zh: "Chinese",
  es: "Spanish",
  fr: "French",
  de: "German",
  multi: "Multilingual",
};

export function formatModelSize(sizeMb: number): string {
  if (!Number.isFinite(sizeMb) || sizeMb < 0) {
    return "—";
  }
  if (sizeMb >= 1024) {
    return `${(sizeMb / 1024).toFixed(1)} GB`;
  }
  return `${sizeMb} MB`;
}

export function formatLanguageList(codes: string[]): string {
  const seen = new Set<string>();
  const labels: string[] = [];
  for (const code of codes) {
    const label = LANGUAGE_LABELS[code] ?? code;
    if (!seen.has(label)) {
      seen.add(label);
      labels.push(label);
    }
  }
  return labels.join(", ");
}

function statusProgress(model: ModelDescriptor) {
  if (typeof model.download_progress === "number") {
    return model.download_progress;
  }

  switch (model.status) {
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

export function modelActionState(model: Pick<ModelDescriptor, "download_url" | "status">) {
  const activeDownload = ["downloading", "verifying", "extracting"].includes(model.status);
  const usableModel = ["ready", "installed", "unloaded"].includes(model.status);

  return {
    canSwitch: usableModel,
    canDownload: Boolean(model.download_url) && model.status === "missing",
    canVerify: !activeDownload,
    canRetry: Boolean(model.download_url) && model.status === "error",
    canCancelDownload: activeDownload,
    canDelete: !activeDownload && model.status !== "missing",
  };
}

export type ModelAcquireKind = "download" | "none";

/// The primary "acquire" action for a model's runtime: download an artifact
/// (whisper-rs `.bin` or sherpa-onnx archive, both carry a `download_url`), or
/// nothing to download (cohere-api is a cloud model that needs a key in Settings).
export function modelAcquireAction(
  model: Pick<ModelDescriptor, "provider_runtime" | "download_url" | "status">,
): { kind: ModelAcquireKind; enabled: boolean; label: string; reason: string | null } {
  if (model.provider_runtime === "cohere-api") {
    return {
      kind: "none",
      enabled: false,
      label: "Download",
      reason: isUsable(model.status)
        ? "Model is already installed"
        : "Set a Cohere API key in Settings",
    };
  }

  const reason = modelDownloadDisabledReason(model);
  return { kind: "download", enabled: reason === null, label: "Download", reason };
}

function isActiveDownload(status: ModelStatus): boolean {
  return status === "downloading" || status === "verifying" || status === "extracting";
}

function isUsable(status: ModelStatus): boolean {
  return status === "ready" || status === "installed" || status === "unloaded";
}

export function modelSwitchDisabledReason(
  model: Pick<ModelDescriptor, "status" | "provider_runtime">,
  isSelected: boolean,
): string | null {
  if (isSelected) return null;
  if (isUsable(model.status)) return null;
  if (isActiveDownload(model.status)) return "Model is still downloading";
  // Cohere is a cloud model — it's acquired by setting an API key, not a download.
  if (model.provider_runtime === "cohere-api" && model.status === "missing") {
    return "Set a Cohere API key in Settings";
  }
  if (model.status === "missing") return "Download the model before switching";
  if (model.status === "error") return "Model has a download error";
  return null;
}

export function modelDownloadDisabledReason(
  model: Pick<ModelDescriptor, "status" | "download_url">,
): string | null {
  if (modelActionState(model).canDownload) return null;
  if (isActiveDownload(model.status)) return "Download already in progress";
  if (!model.download_url) return "No download URL available";
  if (isUsable(model.status)) return "Model is already installed";
  if (model.status === "error") return "Download failed — use Retry";
  return null;
}

export function modelVerifyDisabledReason(
  model: Pick<ModelDescriptor, "status">,
): string | null {
  if (!isActiveDownload(model.status)) return null;
  return "Download in progress";
}

export function modelRetryDisabledReason(
  model: Pick<ModelDescriptor, "status" | "download_url">,
): string | null {
  if (modelActionState(model).canRetry) return null;
  if (isActiveDownload(model.status)) return "Download already in progress";
  if (model.status === "error" && !model.download_url) return "No download URL available";
  return "Nothing to retry";
}

export function modelCancelDownloadDisabledReason(
  model: Pick<ModelDescriptor, "status">,
): string | null {
  if (isActiveDownload(model.status)) return null;
  return "No active download";
}

export function modelDeleteDisabledReason(
  model: Pick<ModelDescriptor, "status">,
  isSelected: boolean,
): string | null {
  if (isSelected) return "Cannot delete the active model";
  if (isActiveDownload(model.status)) return "Download in progress";
  if (model.status === "missing") return "Model is not downloaded";
  return null;
}

export function ModelManager({
  models,
  settings,
  onPatch,
  onVerify,
  onDownload,
  onCancelDownload,
  onDelete,
}: {
  models: ModelDescriptor[];
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
  onVerify: (modelId: string) => void;
  onDownload: (modelId: string) => void;
  onCancelDownload: (modelId: string) => void;
  onDelete: (modelId: string) => void;
}) {
  return (
    <div className="model-list">
      {models.map((model) => {
        const selected = settings.selected_model === model.id;
        const progress = statusProgress(model);
        const actions = modelActionState(model);
        const switchReason = modelSwitchDisabledReason(model, selected);
        const acquire = modelAcquireAction(model);
        const verifyReason = modelVerifyDisabledReason(model);
        const retryReason = modelRetryDisabledReason(model);
        const cancelDownloadReason = modelCancelDownloadDisabledReason(model);
        const deleteReason = modelDeleteDisabledReason(model, selected);
        return (
          <article className="model-row" key={model.id} data-selected={selected}>
            <div className="model-row__main">
              <header>
                <div>
                  <strong>{model.display_name}</strong>
                  <span>
                    {model.engine} · {model.provider_runtime} · {formatModelSize(model.size_mb)}
                  </span>
                </div>
                <Badge tone={modelStatusBadgeTone(model.status)}>{humanizeModelStatus(model.status)}</Badge>
              </header>
              <Progress value={progress} />
              <div className="model-row__scores">
                <span>Speed {model.speed_score}/10</span>
                <span>Accuracy {model.accuracy_score}/10</span>
                <span>{formatLanguageList(model.languages)}</span>
                <span>{model.offline ? "Offline" : "Cloud/API"}</span>
                {model.download_error ? (
                  <span data-tone="danger">{model.download_error}</span>
                ) : null}
              </div>
            </div>
            <div className="model-row__actions">
              <Button
                type="button"
                variant={selected ? "secondary" : "primary"}
                size="sm"
                onClick={() => onPatch({ selected_model: model.id })}
                disabled={selected ? false : !actions.canSwitch}
                title={switchReason ?? undefined}
              >
                <CheckCircle2 data-icon="inline-start" />
                {selected ? "Active" : "Switch"}
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="icon"
                title={acquire.reason ?? acquire.label}
                onClick={() => {
                  if (acquire.kind === "download") {
                    onDownload(model.id);
                  }
                }}
                disabled={!acquire.enabled}
              >
                <Download />
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="icon"
                title={verifyReason ?? "Verify"}
                onClick={() => onVerify(model.id)}
                disabled={!actions.canVerify}
              >
                <ShieldCheck />
              </Button>
              <Button
                type="button"
                variant="secondary"
                size="icon"
                title={retryReason ?? "Retry"}
                onClick={() => onDownload(model.id)}
                disabled={!actions.canRetry}
              >
                <RotateCw />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                title={cancelDownloadReason ?? "Cancel Download"}
                onClick={() => onCancelDownload(model.id)}
                disabled={!actions.canCancelDownload}
              >
                <CircleX />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                title={deleteReason ?? "Delete"}
                onClick={() => onDelete(model.id)}
                disabled={selected || !actions.canDelete}
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
