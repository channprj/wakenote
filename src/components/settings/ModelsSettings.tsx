import { FolderOpenIcon, PlusIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { ModelManager } from "@/components/ModelManager";
import { Button } from "@/components/ui/button";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { StatusBadge } from "@/components/ui/status-badge";
import { Switch } from "@/components/ui/switch";
import { formatModelLabel } from "@/lib/models";
import { modelStatusBadgeTone } from "@/lib/status-summary";
import type { AppSnapshot, DictionaryEntry } from "@/lib/types";
import { SettingSwitch, SettingsCard } from "./settings-controls";
import type { SettingsActions } from "./types";

export function ModelsSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings, models } = snapshot;
  const selectedStatus =
    models.find((model) => model.id === settings.selected_model)?.status ?? "";
  const [newTerm, setNewTerm] = useState("");
  const [newAliases, setNewAliases] = useState("");

  const patchDictionaryEntry = (
    id: string,
    patch: Partial<DictionaryEntry>,
  ) => {
    actions.onPatch({
      dictionary: settings.dictionary.map((entry) =>
        entry.id === id ? { ...entry, ...patch } : entry,
      ),
    });
  };

  return (
    <div className="models-settings">
      <div data-slot="model-files-card">
        <SettingsCard title="Model files" description="Local speech recognition runtime.">
          <Field orientation="responsive" className="settings-row">
            <FieldContent>
              <FieldLabel>Model Directory</FieldLabel>
              <FieldDescription className="overflow-wrap-anywhere">
                Store downloaded model files in this folder.
              </FieldDescription>
            </FieldContent>
            <div className="settings-inline-control">
              <Input
                aria-label="Model Directory"
                value={settings.model_directory}
                title={settings.model_directory}
                onChange={(event) =>
                  actions.onPatch({ model_directory: event.currentTarget.value })
                }
              />
              <Button type="button" size="sm" variant="outline" onClick={actions.onChooseModelDirectory}>
                <FolderOpenIcon data-icon="inline-start" />
                Choose Folder
              </Button>
            </div>
          </Field>
          <Field orientation="horizontal" className="settings-row">
            <FieldContent>
              <FieldLabel>Selected model</FieldLabel>
              <FieldDescription>
                The active model used for new transcription jobs.
              </FieldDescription>
            </FieldContent>
            <StatusBadge tone={modelStatusBadgeTone(selectedStatus)}>
              {formatModelLabel(settings.selected_model, models)}
            </StatusBadge>
          </Field>
        </SettingsCard>
      </div>

      <SettingsCard
        title="Dictionary"
        description="Canonical terms and explicit aliases shared by every transcription model."
      >
        <SettingSwitch
          label="Enable shared Dictionary"
          checked={settings.dictionary_enabled}
          description="Use provider hints when supported and deterministic alias correction everywhere."
          onCheckedChange={(dictionary_enabled) => actions.onPatch({ dictionary_enabled })}
        />
        <Field orientation="vertical" className="settings-row">
          <FieldContent>
            <FieldLabel>Add Dictionary entry</FieldLabel>
            <FieldDescription>
              Aliases are optional and comma-separated. WakeNote only replaces aliases you enter.
            </FieldDescription>
          </FieldContent>
          <div className="settings-inline-control w-full">
            <Input
              aria-label="New canonical term"
              value={newTerm}
              placeholder="WakeNote"
              onChange={(event) => setNewTerm(event.currentTarget.value)}
            />
            <Input
              aria-label="New term aliases"
              value={newAliases}
              placeholder="wake note, wake-note"
              onChange={(event) => setNewAliases(event.currentTarget.value)}
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              aria-label="Add Dictionary entry"
              disabled={!newTerm.trim()}
              onClick={() => {
                const term = newTerm.trim();
                if (!term) return;
                actions.onPatch({
                  dictionary: [
                    ...settings.dictionary,
                    {
                      id: nextDictionaryEntryId(settings.dictionary),
                      term,
                      aliases: parseDictionaryAliases(newAliases, term),
                      enabled: true,
                    },
                  ],
                });
                setNewTerm("");
                setNewAliases("");
              }}
            >
              <PlusIcon data-icon="inline-start" />
              Add
            </Button>
          </div>
        </Field>
        {settings.dictionary.map((entry, index) => (
          <Field
            key={entry.id}
            orientation="vertical"
            className="settings-row"
            data-slot="dictionary-entry"
          >
            <div className="settings-inline-control w-full">
              <Switch
                aria-label={`Enable Dictionary entry ${entry.term}`}
                checked={entry.enabled}
                onCheckedChange={(enabled) => patchDictionaryEntry(entry.id, { enabled })}
              />
              <Input
                aria-label={`Dictionary canonical term ${index + 1}`}
                value={entry.term}
                onChange={(event) => {
                  const term = event.currentTarget.value.trim();
                  if (term) patchDictionaryEntry(entry.id, { term });
                }}
              />
              <Input
                aria-label={`Dictionary aliases ${index + 1}`}
                value={entry.aliases.join(", ")}
                onChange={(event) =>
                  patchDictionaryEntry(entry.id, {
                    aliases: parseDictionaryAliases(
                      event.currentTarget.value,
                      entry.term,
                    ),
                  })
                }
              />
              <Button
                type="button"
                size="icon"
                variant="ghost"
                aria-label={`Delete Dictionary entry ${entry.term}`}
                onClick={() =>
                  actions.onPatch({
                    dictionary: settings.dictionary.filter(
                      (candidate) => candidate.id !== entry.id,
                    ),
                  })
                }
              >
                <Trash2Icon data-icon="solo" />
              </Button>
            </div>
          </Field>
        ))}
      </SettingsCard>

      <ModelManager
        models={models}
        settings={settings}
        onPatch={actions.onPatch}
        onVerify={actions.onVerifyModel}
        onDownload={actions.onDownloadModel}
        onCancelDownload={actions.onCancelModelDownload}
        onDelete={actions.onDeleteModel}
      />
    </div>
  );
}

export function nextDictionaryEntryId(entries: DictionaryEntry[]): string {
  const ids = new Set(entries.map((entry) => entry.id));
  let index = 1;
  while (ids.has(`dictionary-${index}`)) index += 1;
  return `dictionary-${index}`;
}

export function parseDictionaryAliases(value: string, term: string): string[] {
  const canonicalKey = term.trim().toLocaleLowerCase();
  const seen = new Set<string>();
  return value
    .split(",")
    .map((alias) => alias.trim())
    .filter((alias) => {
      const key = alias.toLocaleLowerCase();
      if (!alias || key === canonicalKey || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}
