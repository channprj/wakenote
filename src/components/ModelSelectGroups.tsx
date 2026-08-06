import { groupModelsByLocation } from "@/lib/models";
import type { ModelDescriptor } from "@/lib/types";
import { SelectGroup, SelectItem, SelectLabel } from "./ui/select";

export function ModelSelectGroups({
  models,
}: {
  models: readonly ModelDescriptor[];
}) {
  return groupModelsByLocation(models).map((group) => (
    <SelectGroup key={group.kind} data-model-kind={group.kind}>
      <SelectLabel>{group.label}</SelectLabel>
      {group.models.map((model) => (
        <SelectItem key={model.id} value={model.id}>
          {model.display_name}
        </SelectItem>
      ))}
    </SelectGroup>
  ));
}
