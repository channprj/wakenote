import { StatusBadge } from "@/components/ui/status-badge";
import {
  availableFallbackModels,
  formatModelLabel,
  normalizeTranscriptionOptions,
} from "@/lib/models";
import type { AppSettings, ModelDescriptor } from "@/lib/types";
import {
  SettingSelect,
  SettingSwitch,
  SettingsCard,
} from "./settings-controls";

const NO_FALLBACK = "__none__";

export function transcriptionFallbackOptions(
  models: ModelDescriptor[],
  selectedModelId: string | undefined,
  persistedFallbackId: string | null,
) {
  const ready = availableFallbackModels(models).filter(
    (model) => model.id !== selectedModelId,
  );
  const persistedIsUnavailable = Boolean(
    persistedFallbackId &&
      !ready.some((model) => model.id === persistedFallbackId),
  );
  return {
    ready,
    persistedIsUnavailable,
    options: [
      { value: NO_FALLBACK, label: "None" },
      ...ready.map((model) => ({
        value: model.id,
        label: model.display_name,
      })),
      ...(persistedIsUnavailable && persistedFallbackId
        ? [
            {
              value: persistedFallbackId,
              label: `${formatModelLabel(persistedFallbackId, models)} (Unavailable)`,
            },
          ]
        : []),
    ],
  };
}

export function TranscriptionOptionsSettings({
  settings,
  models,
  onPatch,
}: {
  settings: AppSettings;
  models: ModelDescriptor[];
  onPatch: (patch: Partial<AppSettings>) => void;
}) {
  const selectedModel = models.find(
    (model) => model.id === settings.selected_model,
  );
  const requested = settings.transcription_options;
  const effective = normalizeTranscriptionOptions(
    selectedModel,
    models,
    requested,
  );
  const fallbackChoices = transcriptionFallbackOptions(
    models,
    selectedModel?.id,
    requested.cost_limit_fallback_model_id,
  );
  const fallbackModels = fallbackChoices.ready;
  const fallbackId = requested.cost_limit_fallback_model_id;
  const selectedFallback = fallbackModels.find(
    (model) => model.id === fallbackId,
  );
  const unavailableFallback = fallbackChoices.persistedIsUnavailable;
  const streamingCapability =
    selectedModel?.capabilities.streaming ?? "unsupported";
  const streamingDescription =
    streamingCapability === "required"
      ? "Streaming is required by GPT Live Transcribe and stays on."
      : streamingCapability === "optional"
        ? "Receive partial transcript updates while the API processes audio."
        : "The selected model does not support streaming.";
  const fallbackAllowed = Boolean(selectedModel && !selectedModel.offline);
  const fallbackCanEnable = fallbackAllowed && Boolean(selectedFallback);

  const patchOptions = (next: AppSettings["transcription_options"]) =>
    onPatch({ transcription_options: next });

  return (
    <SettingsCard
      title="Transcription options"
      description="Capability-aware API behavior and on-device recovery."
    >
      <SettingSwitch
        label="Streaming"
        description={streamingDescription}
        checked={effective.streaming_enabled}
        disabled={streamingCapability !== "optional"}
        title={
          streamingCapability === "required"
            ? "Required by this model"
            : streamingCapability === "unsupported"
              ? "Not supported by this model"
              : undefined
        }
        onCheckedChange={(streamingEnabled) =>
          patchOptions({
            ...requested,
            streaming_enabled: streamingEnabled,
          })
        }
      />
      <SettingSwitch
        label="Cost-limit fallback"
        description="Retry once with the selected on-device model only when the API reports an exhausted balance or spend limit."
        checked={effective.cost_limit_fallback_enabled}
        disabled={!fallbackCanEnable}
        title={
          fallbackAllowed
            ? fallbackCanEnable
              ? undefined
              : "Choose an available on-device model first"
            : "Fallback is only used from API models"
        }
        onCheckedChange={(enabled) =>
          patchOptions({
            ...requested,
            cost_limit_fallback_enabled: enabled,
          })
        }
      />
      <SettingSelect
        label="Fallback model"
        description="Only installed, verified, file-capable on-device models are listed."
        value={fallbackId ?? NO_FALLBACK}
        options={fallbackChoices.options}
        disabled={!fallbackAllowed}
        onValueChange={(modelId) =>
          patchOptions({
            ...requested,
            cost_limit_fallback_enabled:
              modelId === NO_FALLBACK
                ? false
                : requested.cost_limit_fallback_enabled,
            cost_limit_fallback_model_id:
              modelId === NO_FALLBACK ? null : modelId,
          })
        }
      />
      {unavailableFallback && fallbackId ? (
        <div className="transcription-fallback-unavailable">
          <span>{formatModelLabel(fallbackId, models)}</span>
          <StatusBadge tone="warning">Unavailable</StatusBadge>
        </div>
      ) : null}
    </SettingsCard>
  );
}
