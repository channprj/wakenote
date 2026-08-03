import {
  AlertCircleIcon,
  FilePenLineIcon,
  PlusIcon,
  RefreshCwIcon,
  XIcon,
} from "lucide-react";
import { useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type {
  AppSettings,
  DictionaryEntry,
  DictionaryFileStatus,
} from "@/lib/types";
import { SettingSwitch, SettingsCard } from "./settings-controls";

interface DictionaryDraft {
  id: string | null;
  term: string;
  aliases: string;
  enabled: boolean;
}

export function DictionarySettingsCard({
  settings,
  status,
  onPatch,
  onOpenFile,
  onReloadFile,
}: {
  settings: AppSettings;
  status: DictionaryFileStatus;
  onPatch: (patch: Partial<AppSettings>) => void | Promise<void>;
  onOpenFile: () => void | Promise<void>;
  onReloadFile: () => void | Promise<void>;
}) {
  const [draft, setDraft] = useState<DictionaryDraft | null>(null);
  const editing = draft?.id !== null;

  function editEntry(entry: DictionaryEntry) {
    setDraft({
      id: entry.id,
      term: entry.term,
      aliases: entry.aliases.join(", "),
      enabled: entry.enabled,
    });
  }

  function saveDraft() {
    if (!draft) return;
    const term = draft.term.trim();
    if (!term) return;
    const nextEntry: DictionaryEntry = {
      id: draft.id ?? nextDictionaryEntryId(settings.dictionary),
      term,
      aliases: parseDictionaryAliases(draft.aliases, term),
      enabled: draft.enabled,
    };
    const dictionary = draft.id
      ? settings.dictionary.map((entry) =>
          entry.id === draft.id ? nextEntry : entry,
        )
      : [...settings.dictionary, nextEntry];
    void onPatch({ dictionary });
    setDraft(null);
  }

  return (
    <>
      <SettingsCard
        title="Dictionary"
        description="Keep canonical terms compact here, or edit many entries together in dictionary.txt."
      >
        <SettingSwitch
          label="Enable shared Dictionary"
          checked={settings.dictionary_enabled}
          description="Use provider hints when supported and deterministic alias correction everywhere."
          onCheckedChange={(dictionary_enabled) =>
            void onPatch({ dictionary_enabled })
          }
        />

        <div className="dictionary-toolbar">
          <Button
            type="button"
            size="sm"
            variant="outline"
            aria-label="Add Dictionary entry"
            onClick={() =>
              setDraft({ id: null, term: "", aliases: "", enabled: true })
            }
          >
            <PlusIcon data-icon="inline-start" />
            Add entry
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            aria-label="Open dictionary.txt"
            onClick={() => void onOpenFile()}
          >
            <FilePenLineIcon data-icon="inline-start" />
            Open dictionary.txt
          </Button>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label="Reload dictionary.txt"
            onClick={() => void onReloadFile()}
          >
            <RefreshCwIcon data-icon="solo" />
          </Button>
        </div>

        {settings.dictionary.length > 0 ? (
          <div
            className="dictionary-chip-cloud"
            aria-label="Dictionary entries"
          >
            {settings.dictionary.map((entry) => (
              <div
                key={entry.id}
                className="dictionary-chip"
                data-disabled={!entry.enabled || undefined}
              >
                <button
                  type="button"
                  className="dictionary-chip__label"
                  aria-label={`Edit Dictionary entry ${entry.term}`}
                  onClick={() => editEntry(entry)}
                >
                  {entry.term}
                </button>
                <button
                  type="button"
                  className="dictionary-chip__delete"
                  aria-label={`Delete Dictionary entry ${entry.term}`}
                  onClick={() =>
                    void onPatch({
                      dictionary: settings.dictionary.filter(
                        (candidate) => candidate.id !== entry.id,
                      ),
                    })
                  }
                >
                  <XIcon aria-hidden="true" />
                </button>
              </div>
            ))}
          </div>
        ) : (
          <p className="dictionary-empty">No Dictionary entries yet.</p>
        )}

        <div className="dictionary-example">
          <span>Input example</span>
          <pre
            role="region"
            aria-label="Dictionary file format example"
            tabIndex={0}
          >
            <code>{`# Canonical term = aliases
WakeNote = wake note, wake-note
Qwen3 ASR = qwen 3 asr, 큐원 ASR`}</code>
          </pre>
        </div>

        {status.error ? (
          <Alert variant="destructive">
            <AlertCircleIcon />
            <AlertDescription>{status.error}</AlertDescription>
          </Alert>
        ) : null}
      </SettingsCard>

      <Dialog
        open={Boolean(draft)}
        onOpenChange={(open) => !open && setDraft(null)}
      >
        {draft ? (
          <DialogContent
            aria-label={
              editing ? "Edit Dictionary entry" : "Add Dictionary entry"
            }
            className="dictionary-dialog"
          >
            <DialogHeader>
              <DialogTitle>
                {editing ? "Edit Dictionary entry" : "Add Dictionary entry"}
              </DialogTitle>
              <DialogDescription>
                Aliases are optional and comma-separated. WakeNote only replaces
                aliases you enter.
              </DialogDescription>
            </DialogHeader>

            <div className="dictionary-dialog__fields">
              <Field orientation="vertical">
                <FieldContent>
                  <FieldLabel htmlFor="dictionary-term">
                    Canonical term
                  </FieldLabel>
                  <FieldDescription>
                    The compact chip label and final spelling.
                  </FieldDescription>
                </FieldContent>
                <Input
                  id="dictionary-term"
                  aria-label="Canonical term"
                  value={draft.term}
                  placeholder="WakeNote"
                  autoFocus
                  onChange={(event) => {
                    const term = event.currentTarget.value;
                    setDraft((current) =>
                      current ? { ...current, term } : current,
                    );
                  }}
                />
              </Field>
              <Field orientation="vertical">
                <FieldContent>
                  <FieldLabel htmlFor="dictionary-aliases">Aliases</FieldLabel>
                  <FieldDescription>
                    Separate alternate spellings with commas.
                  </FieldDescription>
                </FieldContent>
                <Input
                  id="dictionary-aliases"
                  aria-label="Aliases"
                  value={draft.aliases}
                  placeholder="wake note, wake-note"
                  onChange={(event) => {
                    const aliases = event.currentTarget.value;
                    setDraft((current) =>
                      current ? { ...current, aliases } : current,
                    );
                  }}
                />
              </Field>
              <Field
                orientation="horizontal"
                className="dictionary-dialog__enabled"
              >
                <FieldContent>
                  <FieldLabel htmlFor="dictionary-enabled">Enabled</FieldLabel>
                  <FieldDescription>
                    Keep the entry without applying it to new transcripts when
                    off.
                  </FieldDescription>
                </FieldContent>
                <Switch
                  id="dictionary-enabled"
                  aria-label="Enable Dictionary entry"
                  checked={draft.enabled}
                  onCheckedChange={(enabled) =>
                    setDraft((current) =>
                      current ? { ...current, enabled } : current,
                    )
                  }
                />
              </Field>
            </div>

            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                onClick={() => setDraft(null)}
              >
                Cancel
              </Button>
              <Button
                type="button"
                aria-label="Save Dictionary entry"
                disabled={!draft.term.trim()}
                onClick={saveDraft}
              >
                Save
              </Button>
            </DialogFooter>
          </DialogContent>
        ) : null}
      </Dialog>
    </>
  );
}

export function nextDictionaryEntryId(entries: DictionaryEntry[]): string {
  const ids = new Set(entries.map((entry) => entry.id));
  let index = 1;
  while (ids.has(`dictionary-${index}`)) index += 1;
  return `dictionary-${index}`;
}

export function parseDictionaryAliases(value: string, term: string): string[] {
  const canonicalKey = dictionaryComparisonKey(term.trim());
  const seen = new Set<string>();
  return value
    .split(",")
    .map((alias) => alias.trim())
    .filter((alias) => {
      const key = dictionaryComparisonKey(alias);
      if (!alias || key === canonicalKey || seen.has(key)) return false;
      seen.add(key);
      return true;
    });
}

function dictionaryComparisonKey(value: string): string {
  return /^[\x00-\x7F]*$/.test(value) ? value.toLowerCase() : value;
}
