import type { ModelDescriptor } from "./types";

type ModelLabelSource = Pick<ModelDescriptor, "id" | "display_name">;

export function formatModelLabel(modelId: string, models: ModelLabelSource[]): string {
  const match = models.find((model) => model.id === modelId);
  const displayName = match?.display_name?.trim();
  if (displayName) {
    return displayName;
  }
  return modelId;
}
