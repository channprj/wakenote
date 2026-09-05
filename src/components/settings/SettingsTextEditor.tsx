import { useEffect, useId, useRef, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { Spinner } from "@/components/ui/spinner";
import { Textarea } from "@/components/ui/textarea";

export function SettingsTextEditor({
  label,
  description,
  value,
  defaultValue,
  multiline = true,
  rows = 8,
  onSave,
}: {
  label: string;
  description?: string;
  value: string;
  defaultValue?: string;
  multiline?: boolean;
  rows?: number;
  onSave: (value: string) => void | Promise<void>;
}) {
  const id = useId();
  const [draft, setDraft] = useState(value);
  const [baseline, setBaseline] = useState(value);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const savingRef = useRef(false);
  const dirty = draft !== baseline;

  // Only a change from the persisted source may synchronize a clean editor.
  // Draft keystrokes and incoming snapshots must never overwrite one another.
  useEffect(() => {
    if (!dirty && !savingRef.current) {
      setDraft(value);
      setBaseline(value);
    }
  }, [value]);

  async function save() {
    if (savingRef.current || !dirty || !draft.trim()) return;
    savingRef.current = true;
    setSaving(true);
    setError(null);
    try {
      await onSave(draft);
      setBaseline(draft);
      setSaved(true);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  }

  const inputProps = {
    id,
    value: draft,
    disabled: saving,
    "aria-invalid": Boolean(error),
    "aria-describedby": description ? `${id}-description` : undefined,
    onChange: (
      event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>,
    ) => {
      setDraft(event.currentTarget.value);
      setError(null);
      setSaved(false);
    },
  };

  return (
    <Field
      orientation="vertical"
      className="settings-row"
      data-invalid={Boolean(error)}
    >
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      {description ? (
        <FieldDescription id={`${id}-description`}>
          {description}
        </FieldDescription>
      ) : null}
      {multiline ? (
        <Textarea {...inputProps} rows={rows} className="max-h-96 resize-y" />
      ) : (
        <Input {...inputProps} />
      )}
      {dirty && value !== baseline ? (
        <FieldDescription>
          The saved value changed elsewhere. Your draft is preserved.
        </FieldDescription>
      ) : null}
      {error ? (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      ) : null}
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="sm"
          disabled={saving || !dirty || !draft.trim()}
          aria-label={`Save ${label}`}
          onClick={() => void save()}
        >
          {saving ? <Spinner data-icon="inline-start" /> : null}
          {saving ? "Saving…" : "Save"}
        </Button>
        <Button
          size="sm"
          variant="outline"
          disabled={saving || !dirty}
          aria-label={`Cancel ${label} changes`}
          onClick={() => {
            setDraft(value);
            setBaseline(value);
            setError(null);
            setSaved(false);
          }}
        >
          Cancel
        </Button>
        {defaultValue !== undefined ? (
          <Button
            size="sm"
            variant="ghost"
            disabled={saving || draft === defaultValue}
            aria-label={`Reset ${label} to default`}
            onClick={() => {
              setDraft(defaultValue);
              setError(null);
              setSaved(false);
            }}
          >
            Reset to default
          </Button>
        ) : null}
        <FieldDescription role="status">
          {dirty ? "Unsaved changes" : saved ? "Saved" : ""}
        </FieldDescription>
      </div>
    </Field>
  );
}
