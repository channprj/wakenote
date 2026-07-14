import { PlusIcon, Trash2Icon } from "lucide-react";
import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { AppSettings, CustomSourceEntry } from "@/lib/types";

export function CustomSourceSettings({
  settings,
  onPatch,
}: {
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
}) {
  const [drafts, setDrafts] = useState(settings.custom_sources);

  useEffect(() => {
    setDrafts(settings.custom_sources);
  }, [settings.custom_sources]);

  function commit(next: CustomSourceEntry[]) {
    setDrafts(next);
    onPatch({ custom_sources: next });
  }

  return (
    <div className="custom-source-settings">
      {drafts.map((source) => (
        <div className="custom-source-settings__row" key={source.id}>
          <Input
            aria-label="Custom source"
            title={`Source patterns for ${source.label}`}
            value={customSourceText(source)}
            placeholder="Google Meet, Meet, Zoom Meeting"
            onChange={(event) => {
              const patch = parseCustomSourceInput(event.currentTarget.value);
              setDrafts((current) =>
                current.map((candidate) =>
                  candidate.id === source.id ? { ...candidate, ...patch } : candidate,
                ),
              );
            }}
            onBlur={() => commit(drafts)}
          />
          <Switch
            aria-label={`Auto capture ${source.label}`}
            checked={source.auto_prompt}
            onCheckedChange={(auto_prompt) =>
              commit(
                drafts.map((candidate) =>
                  candidate.id === source.id ? { ...candidate, auto_prompt } : candidate,
                ),
              )
            }
          />
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label={`Remove ${source.label}`}
            onClick={() => commit(drafts.filter((candidate) => candidate.id !== source.id))}
          >
            <Trash2Icon data-icon="solo" />
          </Button>
        </div>
      ))}
      <Button
        type="button"
        size="sm"
        variant="outline"
        onClick={() => {
          const id = nextCustomSourceId(drafts);
          commit([
            ...drafts,
            { id, label: "New Source", title_patterns: ["New Source"], auto_prompt: true },
          ]);
        }}
      >
        <PlusIcon data-icon="inline-start" />
        Add Source
      </Button>
    </div>
  );
}

export function customSourceText(source: CustomSourceEntry): string {
  return source.title_patterns.length > 0
    ? source.title_patterns.join(", ")
    : source.label;
}

export function parseCustomSourceInput(
  value: string,
): Pick<CustomSourceEntry, "label" | "title_patterns"> {
  const titlePatterns = value
    .split(",")
    .map((pattern) => pattern.trim())
    .filter(Boolean);
  return {
    label: titlePatterns[0] ?? value.trim(),
    title_patterns: titlePatterns,
  };
}

function nextCustomSourceId(sources: CustomSourceEntry[]): string {
  const used = new Set(sources.map((source) => source.id));
  let index = sources.length + 1;
  let id = `custom-source-${index}`;
  while (used.has(id)) {
    index += 1;
    id = `custom-source-${index}`;
  }
  return id;
}
