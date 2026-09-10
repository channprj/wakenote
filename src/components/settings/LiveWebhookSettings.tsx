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

const URL_DEBOUNCE_MS = 700;
const SAVE_INTERVAL_MS = 1_000;

function normalized(settings: LiveTranscriptionWebhookSettings) {
  return { ...settings, endpoint_url: settings.endpoint_url.trim() };
}

function sameSettings(
  a: LiveTranscriptionWebhookSettings,
  b: LiveTranscriptionWebhookSettings,
) {
  return (
    a.enabled === b.enabled &&
    a.endpoint_url === b.endpoint_url &&
    a.payload_format === b.payload_format &&
    a.auto_delete_history === b.auto_delete_history
  );
}

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
  const [composing, setComposing] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [saveError, setSaveError] = useState(false);
  const mounted = useRef(false);
  const draftRef = useRef(draft);
  const baselineRef = useRef(baseline);
  const inFlight = useRef<LiveTranscriptionWebhookSettings | null>(null);
  const onSaveRef = useRef(onSave);
  const errorRef = useRef(false);
  const composingRef = useRef(false);
  const urlEditedAt = useRef(-Infinity);
  const lastSaveAt = useRef(-Infinity);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  onSaveRef.current = onSave;
  const dirty = !sameSettings(draft, baseline);
  const urlError = attempted ? webhookUrlError(draft) : null;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      clearTimeout(timer.current);
      const next = normalized(draftRef.current);
      // App's settings writer serializes these patches, including across page
      // changes. Enqueue the latest valid edit before a new editor can mount.
      // Never re-enqueue the same in-flight value or retry a known failed edit.
      if (
        !composingRef.current &&
        !errorRef.current &&
        !webhookUrlError(next) &&
        !sameSettings(next, inFlight.current ?? baselineRef.current)
      ) {
        void (async () => {
          await onSaveRef.current({ live_transcription_webhook: next });
        })().catch(() => {});
      }
    };
  }, []);

  useEffect(() => {
    if (!dirty && !savingRef.current) {
      draftRef.current = settings;
      baselineRef.current = settings;
      setDraft(settings);
      setBaseline(settings);
    }
  }, [settings]);

  function patch(change: Partial<LiveTranscriptionWebhookSettings>) {
    draftRef.current = { ...draftRef.current, ...change };
    if (change.endpoint_url !== undefined) {
      urlEditedAt.current = Date.now();
      setAttempted(false);
    }
    setDraft(draftRef.current);
    errorRef.current = false;
    setSaveError(false);
  }

  async function save() {
    if (savingRef.current) return;
    const submitted = draftRef.current;
    setAttempted(true);
    if (webhookUrlError(submitted)) return;
    const next = normalized(submitted);
    if (sameSettings(next, baselineRef.current)) {
      draftRef.current = next;
      setDraft(next);
      return;
    }
    savingRef.current = true;
    inFlight.current = next;
    lastSaveAt.current = Date.now();
    setSaving(true);
    errorRef.current = false;
    setSaveError(false);
    try {
      await onSaveRef.current({ live_transcription_webhook: next });
      if (!mounted.current) return;
      baselineRef.current = next;
      setBaseline(next);
      // Keep text entered while persistence was in progress. Its own debounce
      // and the save interval determine when the newest draft is sent next.
      if (sameSettings(draftRef.current, submitted)) {
        draftRef.current = next;
        setDraft(next);
        setAttempted(false);
      }
    } catch {
      if (mounted.current) {
        // Don't expose paths or endpoint secrets, and don't endlessly retry an
        // unchanged failed edit. A newer draft still gets its scheduled save.
        errorRef.current = sameSettings(draftRef.current, submitted);
        setSaveError(errorRef.current);
      }
    } finally {
      savingRef.current = false;
      inFlight.current = null;
      if (mounted.current) setSaving(false);
    }
  }

  useEffect(() => {
    if (!dirty || saving || saveError || composing) return;
    const due = Math.max(
      urlEditedAt.current + URL_DEBOUNCE_MS,
      lastSaveAt.current + SAVE_INTERVAL_MS,
    );
    timer.current = setTimeout(
      () => void save(),
      Math.max(0, due - Date.now()),
    );
    return () => clearTimeout(timer.current);
  }, [draft, baseline, saving, saveError, composing]);

  return (
    <SettingsCard
      title="Live Transcription Webhook"
      description="Send newly finalized live transcripts to your URL in the background. Each request has a 15-second timeout and up to 3 retries."
    >
      <SettingSwitch
        label="Enable Webhook"
        description="Changes save automatically. Only transcripts finalized after enabling are sent; changing settings does not send a test request."
        checked={draft.enabled}
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
          aria-invalid={Boolean(urlError)}
          aria-describedby={`${id}-help${urlError ? ` ${id}-error` : ""}`}
          onChange={(event) =>
            patch({ endpoint_url: event.currentTarget.value })
          }
          onBlur={() => setAttempted(true)}
          onCompositionStart={() => {
            composingRef.current = true;
            setComposing(true);
          }}
          onCompositionEnd={() => {
            composingRef.current = false;
            urlEditedAt.current = Date.now();
            setComposing(false);
          }}
        />
        <FieldDescription id={`${id}-help`}>
          {draft.endpoint_url.trim().toLowerCase().startsWith("http://")
            ? "HTTP sends transcripts without encryption. Use HTTPS for external services."
            : "HTTPS is recommended. HTTP is also supported for local and internal services."}{" "}
          The URL saves automatically after you pause typing.
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
      <SettingSwitch
        label="Automatically delete old webhook history"
        description="Delete history and saved request content after 24 hours. When off, older history stays on this Mac but remains hidden in Webhooks."
        checked={draft.auto_delete_history}
        onCheckedChange={(auto_delete_history) =>
          patch({ auto_delete_history })
        }
      />
      <div className="settings-row space-y-3">
        <p className="text-xs text-muted-foreground">
          Enabling sends transcript content to the configured endpoint. JSON
          also sends metadata. Delivery and order are not guaranteed. Receivers
          can use X-Transcription-Event-Id to handle duplicates. Disabling or
          changing the endpoint or format cancels unfinished deliveries; data
          already sent cannot be recalled. Requests are saved locally for manual
          retries in Webhooks. Changing automatic deletion does not interrupt
          delivery.
        </p>
        {saveError ? (
          <p role="alert" className="text-sm text-destructive">
            Could not save webhook settings. Your changes are preserved; try
            again.
          </p>
        ) : null}
        {dirty && !sameSettings(settings, baseline) ? (
          <p className="text-xs text-muted-foreground">
            The saved settings changed elsewhere. Your draft is preserved.
          </p>
        ) : null}
        <p role="status" className="text-xs text-muted-foreground">
          {saving
            ? "Saving…"
            : saveError
              ? "Changes not saved."
              : dirty
                ? urlError
                  ? "Enter a valid endpoint to save your changes."
                  : "Waiting to save…"
                : "Settings saved automatically."}
        </p>
        {saveError ? (
          <Button
            size="sm"
            variant="outline"
            onClick={() => {
              errorRef.current = false;
              setSaveError(false);
            }}
          >
            Retry saving
          </Button>
        ) : null}
      </div>
    </SettingsCard>
  );
}
