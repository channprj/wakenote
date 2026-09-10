import { useEffect, useId, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { webhookUrlError } from "@/lib/live-webhook";
import type {
  AppSettings,
  LiveTranscriptionWebhookSettings,
} from "@/lib/types";
import {
  SettingSelect,
  SettingSwitch,
  SettingsCard,
} from "./settings-controls";

export function LiveWebhookSettings({
  settings,
  onSave,
}: {
  settings: LiveTranscriptionWebhookSettings;
  onSave: (patch: Partial<AppSettings>) => void | Promise<void>;
}) {
  const id = useId();
  const [draft, setDraft] = useState(settings);
  const [baseline, setBaseline] = useState(settings);
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const [attempted, setAttempted] = useState(false);
  const [saveError, setSaveError] = useState(false);
  const dirty = JSON.stringify(draft) !== JSON.stringify(baseline);
  const urlError = attempted ? webhookUrlError(draft) : null;

  useEffect(() => {
    if (!dirty && !savingRef.current) {
      setDraft(settings);
      setBaseline(settings);
    }
  }, [settings]);

  function patch(change: Partial<LiveTranscriptionWebhookSettings>) {
    setDraft((current) => ({ ...current, ...change }));
    setSaveError(false);
  }

  async function save() {
    if (savingRef.current) return;
    setAttempted(true);
    if (webhookUrlError(draft)) return;
    const next = { ...draft, endpoint_url: draft.endpoint_url.trim() };
    savingRef.current = true;
    setSaving(true);
    setSaveError(false);
    try {
      await onSave({ live_transcription_webhook: next });
      setDraft(next);
      setBaseline(next);
      setAttempted(false);
    } catch {
      // Persistence errors may contain paths or endpoint secrets. Keep them
      // out of this UI and preserve the draft for another save attempt.
      setSaveError(true);
    } finally {
      savingRef.current = false;
      setSaving(false);
    }
  }

  return (
    <SettingsCard
      title="Live Transcription Webhook"
      description="Send newly finalized live transcripts to your URL in the background. Each request has a 15-second timeout and up to 3 retries."
    >
      <SettingSwitch
        label="Enable Webhook"
        description="Save to apply. Only transcripts finalized after enabling are sent; saving does not send a test request."
        checked={draft.enabled}
        disabled={saving}
        onCheckedChange={(enabled) => patch({ enabled })}
      />
      <Field className="settings-row" data-invalid={Boolean(urlError)}>
        <FieldLabel htmlFor={id}>Endpoint URL</FieldLabel>
        <Input
          id={id}
          type="url"
          placeholder="https://example.com/transcripts"
          autoComplete="off"
          spellCheck={false}
          value={draft.endpoint_url}
          disabled={saving}
          aria-invalid={Boolean(urlError)}
          aria-describedby={`${id}-help${urlError ? ` ${id}-error` : ""}`}
          onChange={(event) =>
            patch({ endpoint_url: event.currentTarget.value })
          }
          onBlur={() => setAttempted(true)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && dirty) {
              event.preventDefault();
              void save();
            }
          }}
        />
        <FieldDescription id={`${id}-help`}>
          {draft.endpoint_url.trim().toLowerCase().startsWith("http://")
            ? "HTTP sends transcripts without encryption. Use HTTPS for external services."
            : "HTTPS is recommended. HTTP is also supported for local and internal services."}
        </FieldDescription>
        {urlError ? (
          <p
            id={`${id}-error`}
            role="alert"
            className="text-sm text-destructive"
          >
            {urlError}
          </p>
        ) : null}
      </Field>
      <SettingSelect
        label="Payload Format"
        value={draft.payload_format}
        disabled={saving}
        options={[
          { value: "text_only", label: "Text Only" },
          { value: "json", label: "JSON" },
        ]}
        onValueChange={(payload_format) => patch({ payload_format })}
        description={
          draft.payload_format === "json"
            ? "JSON includes the transcript and metadata from the existing saved JSON format. No audio is uploaded."
            : "Send only the finalized text, preserving punctuation and line breaks."
        }
      />
      <div className="settings-row space-y-3">
        <p className="text-xs text-muted-foreground">
          Enabling sends transcript content to the configured endpoint. JSON
          also sends metadata. Delivery and order are not guaranteed. Receivers
          can use X-Transcription-Event-Id to handle duplicates. Disabling or
          changing settings cancels unfinished deliveries; data already sent
          cannot be recalled.
        </p>
        {saveError ? (
          <p role="alert" className="text-sm text-destructive">
            Could not save webhook settings. Your changes are preserved; try
            again.
          </p>
        ) : null}
        {dirty && JSON.stringify(settings) !== JSON.stringify(baseline) ? (
          <p className="text-xs text-muted-foreground">
            The saved settings changed elsewhere. Your draft is preserved.
          </p>
        ) : null}
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            disabled={!dirty || saving}
            onClick={() => void save()}
          >
            {saving ? "Saving…" : "Save webhook settings"}
          </Button>
          <Button
            size="sm"
            variant="outline"
            disabled={!dirty || saving}
            onClick={() => {
              setDraft(settings);
              setBaseline(settings);
              setAttempted(false);
              setSaveError(false);
            }}
          >
            Cancel changes
          </Button>
        </div>
      </div>
    </SettingsCard>
  );
}
