import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import {
  dictationShortcutFromKeyboardEvent,
  formatDictationShortcut,
} from "@/lib/dictation-shortcut";
import type { AppSettings, TranscriptionLanguage } from "@/lib/types";
import {
  SettingSelect,
  SettingSwitch,
  SettingsCard,
  SettingsGrid,
  type SelectOption,
} from "./settings-controls";
import type { SettingsActions } from "./types";

const DICTATION_LANGUAGES = [
  { value: "auto", label: "Auto-detect" },
  { value: "ko", label: "Korean" },
  { value: "en", label: "English" },
  { value: "ja", label: "Japanese" },
  { value: "zh", label: "Chinese" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
] as const satisfies ReadonlyArray<SelectOption<TranscriptionLanguage>>;

export function DictationSettings({
  settings,
  actions,
}: {
  settings: AppSettings;
  actions: SettingsActions;
}) {
  const [capturingShortcut, setCapturingShortcut] = useState(false);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const mountedRef = useRef(true);
  const captureActiveRef = useRef(false);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (captureActiveRef.current) {
        captureActiveRef.current = false;
        void actions.onResumeDictationShortcut();
      }
    };
  }, [actions.onResumeDictationShortcut]);

  async function startShortcutCapture() {
    if (
      !settings.dictation_enabled ||
      capturingShortcut ||
      captureActiveRef.current
    ) {
      return;
    }
    setShortcutError(null);
    try {
      await actions.onSuspendDictationShortcut();
    } catch (error) {
      setShortcutError(
        error instanceof Error ? error.message : String(error),
      );
      return;
    }
    if (!mountedRef.current) {
      await actions.onResumeDictationShortcut();
      return;
    }
    captureActiveRef.current = true;
    setCapturingShortcut(true);
  }

  async function cancelShortcutCapture() {
    captureActiveRef.current = false;
    setCapturingShortcut(false);
    setShortcutError(null);
    try {
      await actions.onResumeDictationShortcut();
    } catch (error) {
      setShortcutError(
        error instanceof Error ? error.message : String(error),
      );
    }
  }

  async function commitShortcut(shortcut: string) {
    if (!captureActiveRef.current) {
      return;
    }
    captureActiveRef.current = false;
    setCapturingShortcut(false);
    setShortcutError(null);
    try {
      await actions.onPatch({ dictation_shortcut: shortcut });
    } catch (error) {
      setShortcutError(
        error instanceof Error ? error.message : String(error),
      );
    } finally {
      try {
        await actions.onResumeDictationShortcut();
      } catch (error) {
        setShortcutError(
          error instanceof Error ? error.message : String(error),
        );
      }
    }
  }

  useEffect(() => {
    if (!capturingShortcut) {
      return;
    }
    let cancelled = false;
    let polling = false;
    const poll = async () => {
      if (polling) {
        return;
      }
      polling = true;
      try {
        const shortcut = await actions.onPressedModifierShortcut();
        if (!cancelled && shortcut) {
          await commitShortcut(shortcut);
        }
      } catch (error) {
        if (!cancelled) {
          setShortcutError(
            error instanceof Error ? error.message : String(error),
          );
        }
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), 30);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [capturingShortcut, actions.onPressedModifierShortcut]);

  async function captureShortcut(event: KeyboardEvent<HTMLButtonElement>) {
    if (!capturingShortcut || !captureActiveRef.current) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();

    if (event.code === "Escape" || event.key === "Escape") {
      await cancelShortcutCapture();
      return;
    }

    const shortcut = dictationShortcutFromKeyboardEvent(event);
    if (!shortcut) {
      setShortcutError(
        "Use two modifiers, a modifier with a supported key, or an F-key.",
      );
      return;
    }

    await commitShortcut(shortcut);
  }

  return (
    <SettingsGrid maxColumns={1}>
      <SettingsCard
        title="Shortcut dictation"
        description="Press once to record, then press again to transcribe and type at the cursor."
      >
        <SettingSwitch
          label="Enable shortcut dictation"
          description="Disabled by default. WakeNote only listens for the shortcut when enabled."
          checked={settings.dictation_enabled}
          onCheckedChange={(dictation_enabled) =>
            void actions.onPatch({ dictation_enabled })
          }
        />
        <Field
          orientation="responsive"
          className="settings-row"
          data-disabled={!settings.dictation_enabled || undefined}
          data-invalid={Boolean(shortcutError) || undefined}
        >
          <FieldContent>
            <FieldLabel>Dictation shortcut</FieldLabel>
            <FieldDescription>
              {shortcutError ??
                "Press two modifiers, or a modifier and key. F-keys work alone."}
            </FieldDescription>
          </FieldContent>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className="min-w-40 max-w-full"
            aria-label="Dictation shortcut"
            aria-invalid={Boolean(shortcutError)}
            disabled={!settings.dictation_enabled}
            onClick={() => void startShortcutCapture()}
            onKeyDown={(event) => void captureShortcut(event)}
            onBlur={() => {
              if (capturingShortcut) {
                void cancelShortcutCapture();
              }
            }}
          >
            {capturingShortcut
              ? "Press shortcut…"
              : formatDictationShortcut(settings.dictation_shortcut)}
          </Button>
        </Field>
        <SettingSelect
          label="Dictation language"
          description="Auto-detect is independent of the archival transcription language."
          value={settings.dictation_language}
          options={DICTATION_LANGUAGES}
          disabled={!settings.dictation_enabled}
          onValueChange={(dictation_language) =>
            void actions.onPatch({ dictation_language })
          }
        />
      </SettingsCard>
    </SettingsGrid>
  );
}
