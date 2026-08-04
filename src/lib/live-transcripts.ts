import type { TranscriptEntry } from "./transcript-log";
import { formatModelLabel } from "./models";
import type { ModelDescriptor } from "./types";

export const RECENT_TRANSCRIPT_LIMIT = 3;

export function newestTranscriptTextEntries(
  entries: readonly TranscriptEntry[],
): TranscriptEntry[] {
  const seen = new Set<string>();
  const visibleEntries: TranscriptEntry[] = [];

  for (const entry of [...entries].reverse()) {
    if (entry.status === "failed") {
      continue;
    }

    const text = entry.text.trim();
    if (!text) {
      continue;
    }

    const sourceTextKey = `${entry.source_key ?? "microphone"}:${text}`;
    const key = entry.audio_path ?? sourceTextKey;
    if (seen.has(key) || seen.has(sourceTextKey)) {
      continue;
    }

    seen.add(key);
    seen.add(sourceTextKey);
    visibleEntries.push(entry);
    if (visibleEntries.length === RECENT_TRANSCRIPT_LIMIT) {
      break;
    }
  }

  return visibleEntries.reverse();
}

export function newestLiveTranscriptEntries(
  entries: readonly TranscriptEntry[],
): TranscriptEntry[] {
  const seen = new Set<string>();
  const visibleEntries: TranscriptEntry[] = [];

  for (const entry of [...entries].reverse()) {
    const audioPath = entry.audio_path?.trim();
    const key =
      audioPath ||
      `${entry.source_key ?? "microphone"}:${entry.chunk_id}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    visibleEntries.push(entry);
    if (visibleEntries.length === RECENT_TRANSCRIPT_LIMIT) {
      break;
    }
  }

  return visibleEntries.reverse();
}

export function livePreviewPresentation(
  selectedModelId: string,
  models: readonly ModelDescriptor[],
): {
  previewModelId: string | null;
  provisional: boolean;
  description: string;
} {
  const selected = models.find((model) => model.id === selectedModelId);
  const selectedLabel = formatModelLabel(selectedModelId, [...models]);
  if (selected && isReadyLivePreviewModel(selected)) {
    return {
      previewModelId: selected.id,
      provisional: false,
      description: `Live preview and final transcription use ${selectedLabel}.`,
    };
  }

  const preview = models
    .filter(
      (model) =>
        model.offline &&
        model.provider_runtime === "whisper-rs" &&
        isReadyModel(model),
    )
    .sort(
      (left, right) =>
        right.speed_score - left.speed_score ||
        right.accuracy_score - left.accuracy_score ||
        left.id.localeCompare(right.id),
    )[0];
  if (preview) {
    return {
      previewModelId: preview.id,
      provisional: true,
      description: `Live preview uses ${formatModelLabel(preview.id, [...models])}; final transcription remains ${selectedLabel}.`,
    };
  }

  return {
    previewModelId: null,
    provisional: false,
    description: `Live preview waits for a compatible model; final transcription uses ${selectedLabel}.`,
  };
}

function isReadyModel(model: ModelDescriptor) {
  return ["ready", "installed", "unloaded"].includes(model.status);
}

function isReadyLivePreviewModel(model: ModelDescriptor) {
  return (
    isReadyModel(model) &&
    (model.id === "openai-gpt-live-transcribe" ||
      model.provider_runtime === "whisper-rs")
  );
}
