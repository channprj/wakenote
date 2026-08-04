import {
  CheckCircle2,
  CircleX,
  Download,
  RotateCw,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { Button } from "./ui/button";
import { Progress } from "./ui/progress";
import { StatusBadge } from "./ui/status-badge";
import { modelStatusBadgeTone } from "../lib/status-summary";
import { humanizeModelStatus } from "../lib/transcript-history";
import type { AppSettings, ModelDescriptor, ModelStatus } from "../lib/types";
import { useMasonryGrid } from "../hooks/use-masonry-grid";

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

export function cloudCredentialLabel(providerRuntime: string): string | null {
  switch (providerRuntime) {
    case "openrouter-stt":
      return "Requires OpenRouter API key";
    case "openai-stt":
    case "openai-realtime":
      return "Requires OpenAI API key";
    case "soniox-async-stt":
    case "soniox-realtime":
      return "Requires Soniox API key";
    default:
      return null;
  }
}

export function modelCapabilityLabels(model: ModelDescriptor): string[] {
  const labels: string[] = [];
  if (model.capabilities.realtime) {
    labels.push("Realtime");
  }
  if (model.capabilities.streaming === "required") {
    labels.push("Streaming required");
  } else if (model.capabilities.streaming === "optional") {
    labels.push("Streaming");
  }
  if (model.capabilities.diarization) {
    labels.push("Speaker separation");
  }
  labels.push(model.offline ? "On-device" : "API");
  return labels;
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

export function modelActionState(
  model: Pick<ModelDescriptor, "download_url" | "status"> &
    Partial<Pick<ModelDescriptor, "provider_runtime">>,
) {
  const activeDownload = ["downloading", "verifying", "extracting"].includes(
    model.status,
  );
  const usableModel = ["ready", "installed", "unloaded"].includes(model.status);
  const canSetUpQwen = model.provider_runtime === "qwen3-asr";

  return {
    canSwitch: usableModel,
    canDownload:
      (Boolean(model.download_url) || canSetUpQwen) &&
      model.status === "missing",
    canVerify: !activeDownload,
    canRetry:
      (Boolean(model.download_url) || canSetUpQwen) && model.status === "error",
    canCancelDownload: activeDownload,
    canDelete: !activeDownload && model.status !== "missing",
  };
}

export type ModelAcquireKind = "download" | "none";

/// The primary "acquire" action for a model's runtime: download an artifact
/// (whisper-rs `.bin` or sherpa-onnx archive, both carry a `download_url`).
export function modelAcquireAction(
  model: Pick<ModelDescriptor, "provider_runtime" | "download_url" | "status">,
): {
  kind: ModelAcquireKind;
  enabled: boolean;
  label: string;
  reason: string | null;
} {
  const reason = modelDownloadDisabledReason(model);
  return {
    kind: "download",
    enabled: reason === null,
    label:
      model.provider_runtime === "qwen3-asr" ? "Set up Qwen3-ASR" : "Download",
    reason,
  };
}

function isActiveDownload(status: ModelStatus): boolean {
  return (
    status === "downloading" ||
    status === "verifying" ||
    status === "extracting"
  );
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
  if (model.status === "missing") return "Download the model before switching";
  if (model.status === "error") return "Model has a download error";
  return null;
}

export function modelDownloadDisabledReason(
  model: Pick<ModelDescriptor, "status" | "download_url"> &
    Partial<Pick<ModelDescriptor, "provider_runtime">>,
): string | null {
  if (modelActionState(model).canDownload) return null;
  if (isActiveDownload(model.status)) return "Download already in progress";
  if (!model.download_url && model.provider_runtime !== "qwen3-asr") {
    return "No download URL available";
  }
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
  model: Pick<ModelDescriptor, "status" | "download_url"> &
    Partial<Pick<ModelDescriptor, "provider_runtime">>,
): string | null {
  if (modelActionState(model).canRetry) return null;
  if (isActiveDownload(model.status)) return "Download already in progress";
  if (
    model.status === "error" &&
    !model.download_url &&
    model.provider_runtime !== "qwen3-asr"
  ) {
    return "No download URL available";
  }
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
  isInUse: boolean,
): string | null {
  if (isInUse) {
    return "Cannot delete a model used by transcription or Dictation";
  }
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
  const modelIdentity = JSON.stringify(models.map((model) => model.id));
  const masonryRef = useMasonryGrid<HTMLDivElement>(
    models.length,
    modelIdentity,
  );
  const maxColumns = Math.max(1, Math.min(models.length, 3));

  return (
    <div
      ref={masonryRef}
      className="model-list"
      data-slot="model-card-grid"
      data-max-columns={maxColumns}
    >
      {models.map((model) => {
        const selected = settings.selected_model === model.id;
        const usedByDictation = settings.dictation_model.trim() === model.id;
        const inUse = selected || usedByDictation;
        const progress = statusProgress(model);
        const actions = modelActionState(model);
        const switchReason = modelSwitchDisabledReason(model, selected);
        const acquire = modelAcquireAction(model);
        const verifyReason = modelVerifyDisabledReason(model);
        const retryReason = modelRetryDisabledReason(model);
        const cancelDownloadReason = modelCancelDownloadDisabledReason(model);
        const deleteReason = modelDeleteDisabledReason(model, inUse);
        const credentialLabel = cloudCredentialLabel(model.provider_runtime);
        const capabilityLabels = modelCapabilityLabels(model);
        return (
          <article
            className="model-row"
            key={model.id}
            data-selected={selected}
          >
            <div className="model-row__main">
              <header>
                <div>
                  <strong>{model.display_name}</strong>
                  <span>
                    {model.engine} · {model.provider_runtime} ·{" "}
                    {model.offline ? formatModelSize(model.size_mb) : "API"}
                  </span>
                </div>
                <StatusBadge tone={modelStatusBadgeTone(model.status)}>
                  {humanizeModelStatus(model.status)}
                </StatusBadge>
              </header>
              <Progress
                value={progress}
                aria-label={`${model.display_name} installation progress`}
              />
              <div
                className="model-row__capabilities"
                aria-label="Model capabilities"
                role="list"
              >
                {capabilityLabels.map((label) => (
                  <span key={label} role="listitem">
                    {label}
                  </span>
                ))}
              </div>
              <div className="model-row__scores">
                <span>Speed {model.speed_score}/10</span>
                <span>Accuracy {model.accuracy_score}/10</span>
                <span>{formatLanguageList(model.languages)}</span>
                {credentialLabel ? <span>{credentialLabel}</span> : null}
                {model.download_error ? (
                  <span data-tone="danger">{model.download_error}</span>
                ) : null}
              </div>
            </div>
            <div className="model-row__actions">
              <Button
                type="button"
                variant={selected ? "secondary" : "default"}
                size="sm"
                onClick={() => onPatch({ selected_model: model.id })}
                disabled={selected ? false : !actions.canSwitch}
                title={switchReason ?? undefined}
              >
                <CheckCircle2 data-icon="inline-start" />
                {selected ? "Active" : "Switch"}
              </Button>
              {model.offline ? (
                <>
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
                    <Download data-icon="solo" />
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    size="icon"
                    title={verifyReason ?? "Verify"}
                    onClick={() => onVerify(model.id)}
                    disabled={!actions.canVerify}
                  >
                    <ShieldCheck data-icon="solo" />
                  </Button>
                  <Button
                    type="button"
                    variant="secondary"
                    size="icon"
                    title={retryReason ?? "Retry"}
                    onClick={() => onDownload(model.id)}
                    disabled={!actions.canRetry}
                  >
                    <RotateCw data-icon="solo" />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    title={cancelDownloadReason ?? "Cancel Download"}
                    onClick={() => onCancelDownload(model.id)}
                    disabled={!actions.canCancelDownload}
                  >
                    <CircleX data-icon="solo" />
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon"
                    title={deleteReason ?? "Delete"}
                    onClick={() => onDelete(model.id)}
                    disabled={inUse || !actions.canDelete}
                  >
                    <Trash2 data-icon="solo" />
                  </Button>
                </>
              ) : null}
            </div>
          </article>
        );
      })}
    </div>
  );
}
