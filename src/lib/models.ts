import type {
  ModelDescriptor,
  TranscriptionContext,
  TranscriptionOptions,
} from "./types";

type ModelLabelSource = Pick<ModelDescriptor, "id" | "display_name">;

export interface ModelLocationGroup<T> {
  kind: "local" | "api";
  label: "Local models" | "API models";
  models: T[];
}

export function groupModelsByLocation<
  T extends Pick<ModelDescriptor, "offline">,
>(models: readonly T[]): ModelLocationGroup<T>[] {
  const local = models.filter((model) => model.offline);
  const api = models.filter((model) => !model.offline);
  return [
    { kind: "local", label: "Local models", models: local },
    { kind: "api", label: "API models", models: api },
  ].filter((group) => group.models.length > 0) as ModelLocationGroup<T>[];
}

export function formatModelLabel(
  modelId: string,
  models: ModelLabelSource[],
): string {
  const match = models.find((model) => model.id === modelId);
  const displayName = match?.display_name?.trim();
  if (displayName) {
    return displayName;
  }
  return modelId;
}

export function modelSupportsContext(
  model: ModelDescriptor,
  context: TranscriptionContext,
  diarizationRequired = false,
) {
  return (
    model.capabilities.selectable_contexts.includes(context) &&
    (!diarizationRequired || model.capabilities.diarization)
  );
}

export function isReadyOnDeviceFallback(model: ModelDescriptor) {
  return (
    model.offline &&
    model.capabilities.file_transcription &&
    (model.status === "ready" || model.status === "installed")
  );
}

export function availableFallbackModels(models: ModelDescriptor[]) {
  return models.filter(isReadyOnDeviceFallback);
}

export function normalizeTranscriptionOptions(
  model: ModelDescriptor | undefined,
  models: ModelDescriptor[],
  requested: TranscriptionOptions,
): TranscriptionOptions {
  const normalized = { ...requested };
  if (!model) {
    return {
      ...normalized,
      streaming_enabled: false,
      cost_limit_fallback_enabled: false,
    };
  }

  if (model.capabilities.streaming === "required") {
    normalized.streaming_enabled = true;
  } else if (model.capabilities.streaming === "unsupported") {
    normalized.streaming_enabled = false;
  }

  const fallback = models.find(
    (candidate) =>
      candidate.id === requested.cost_limit_fallback_model_id &&
      candidate.id !== model.id,
  );
  const fallbackAvailable =
    !model.offline &&
    Boolean(
      fallback &&
        isReadyOnDeviceFallback(fallback) &&
        (!model.capabilities.diarization || fallback.capabilities.diarization),
    );
  normalized.cost_limit_fallback_enabled =
    requested.cost_limit_fallback_enabled && fallbackAvailable;
  return normalized;
}
