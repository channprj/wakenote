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
import type {
  AppSnapshot,
  DictationBubblePosition,
  DictationCueSound,
  DictationCueVolume,
  TranscriptionLanguage,
} from "@/lib/types";
import {
  SettingSelect,
  SettingSlider,
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

const DICTATION_START_CUE_SOUNDS = [
  { value: "original", label: "Default" },
  { value: "alternative", label: "Ding" },
] as const satisfies ReadonlyArray<SelectOption<DictationCueSound>>;

const DICTATION_STOP_CUE_SOUNDS = [
  { value: "original", label: "Default" },
  { value: "alternative", label: "Ding" },
] as const satisfies ReadonlyArray<SelectOption<DictationCueSound>>;

const DICTATION_END_CUE_SOUNDS = [
  { value: "original", label: "Default" },
  { value: "alternative", label: "Stapling" },
] as const satisfies ReadonlyArray<SelectOption<DictationCueSound>>;

const DICTATION_CUE_VOLUMES = [
  { value: "muted", label: "Muted" },
  { value: "small", label: "Small" },
  { value: "medium", label: "Medium" },
  { value: "large", label: "Large" },
] as const satisfies ReadonlyArray<SelectOption<DictationCueVolume>>;

const DICTATION_BUBBLE_POSITIONS = [
  { value: "top_left", label: "Top left" },
  { value: "top_center", label: "Top center" },
  { value: "top_right", label: "Top right" },
  { value: "bottom_left", label: "Bottom left" },
  { value: "bottom_center", label: "Bottom center" },
  { value: "bottom_right", label: "Bottom right" },
] as const satisfies ReadonlyArray<SelectOption<DictationBubblePosition>>;

const DEFAULT_DICTATION_MODEL = "__default__";
const USABLE_MODEL_STATUSES = new Set(["ready", "installed", "unloaded"]);

function isPhysicalModifierShortcut(shortcut: string): boolean {
  return /^(?:left|right)(?:ctrl|alt|shift|cmd)$/.test(shortcut);
}

export function DictationSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings } = snapshot;
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
        "Press a key, a modified key, a modifier chord, or one physical modifier.",
      );
      return;
    }
    if (isPhysicalModifierShortcut(shortcut)) {
      return;
    }

    await commitShortcut(shortcut);
  }

  async function capturePhysicalModifierRelease(
    event: KeyboardEvent<HTMLButtonElement>,
  ) {
    if (!capturingShortcut || !captureActiveRef.current) {
      return;
    }
    const shortcut = dictationShortcutFromKeyboardEvent(event);
    if (!shortcut || !isPhysicalModifierShortcut(shortcut)) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    await commitShortcut(shortcut);
  }

  const dictationModelOptions: SelectOption<string>[] = [
    {
      value: DEFAULT_DICTATION_MODEL,
      label: "Default transcription model",
    },
    ...snapshot.models
      .filter((model) => USABLE_MODEL_STATUSES.has(model.status))
      .map((model) => ({
        value: model.id,
        label: model.display_name,
      })),
  ];

  return (
    <SettingsGrid maxColumns={2}>
      <SettingsCard
        title="Shortcut dictation"
        description="Hold the shortcut to record. Release it to transcribe and type at the cursor."
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
                "Use a single key, a modified key, a modifier chord, or one physical modifier."}
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
            onKeyUp={(event) => void capturePhysicalModifierRelease(event)}
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
        <SettingSelect
          label="Dictation model"
          description="Use the default transcription model or choose an installed model only for Dictation."
          value={settings.dictation_model || DEFAULT_DICTATION_MODEL}
          options={dictationModelOptions}
          disabled={!settings.dictation_enabled}
          onValueChange={(dictation_model) =>
            void actions.onPatch({
              dictation_model:
                dictation_model === DEFAULT_DICTATION_MODEL
                  ? ""
                  : dictation_model,
            })
          }
        />
        <SettingSwitch
          label="Copy result to clipboard"
          description={
            settings.dictation_copy_to_clipboard
              ? "Keep each pasted Dictation result in the clipboard."
              : "Paste the result and restore the previous clipboard."
          }
          checked={settings.dictation_copy_to_clipboard}
          disabled={!settings.dictation_enabled}
          onCheckedChange={(dictation_copy_to_clipboard) =>
            void actions.onPatch({ dictation_copy_to_clipboard })
          }
        />
        <SettingSwitch
          label="Remove trailing space"
          description="Trim whitespace from the end of Dictation text before pasting."
          checked={settings.dictation_remove_trailing_space}
          disabled={!settings.dictation_enabled}
          onCheckedChange={(dictation_remove_trailing_space) =>
            void actions.onPatch({ dictation_remove_trailing_space })
          }
        />
      </SettingsCard>
      <SettingsCard
        title="Feedback"
        description="Choose the audio cues and where the compact Dictation bubble appears."
      >
        <SettingSelect
          label="Start sound"
          description="Played when shortcut recording starts."
          value={settings.dictation_start_sound}
          options={DICTATION_START_CUE_SOUNDS}
          disabled={!settings.dictation_enabled}
          onValueChange={(dictation_start_sound) =>
            void actions.onPatch({ dictation_start_sound })
          }
        />
        <SettingSelect
          label="Stop sound"
          description="Played when shortcut recording stops."
          value={settings.dictation_stop_sound}
          options={DICTATION_STOP_CUE_SOUNDS}
          disabled={!settings.dictation_enabled}
          onValueChange={(dictation_stop_sound) =>
            void actions.onPatch({ dictation_stop_sound })
          }
        />
        <SettingSelect
          label="End sound"
          description="Played after the Dictation result is inserted."
          value={settings.dictation_end_sound}
          options={DICTATION_END_CUE_SOUNDS}
          disabled={!settings.dictation_enabled}
          onValueChange={(dictation_end_sound) =>
            void actions.onPatch({ dictation_end_sound })
          }
        />
        <SettingSelect
          label="Cue volume"
          description="Mute the cues or play them at a small, medium, or large level."
          value={settings.dictation_cue_volume}
          options={DICTATION_CUE_VOLUMES}
          disabled={!settings.dictation_enabled}
          onValueChange={(dictation_cue_volume) =>
            void actions.onPatch({ dictation_cue_volume })
          }
        />
        <SettingSelect
          label="Bubble position"
          description="Place Dictation feedback at one of six screen edges."
          value={settings.dictation_bubble_position}
          options={DICTATION_BUBBLE_POSITIONS}
          disabled={!settings.dictation_enabled}
          onValueChange={(dictation_bubble_position) =>
            void actions.onPatch({ dictation_bubble_position })
          }
        />
        <SettingSelect
          label="Bubble background"
          description="Use a solid black or white Dictation surface."
          value={settings.dictation_bubble_background_color}
          options={[
            { value: "#000000", label: "Black" },
            { value: "#ffffff", label: "White" },
          ]}
          onValueChange={(dictation_bubble_background_color) =>
            void actions.onPatch({ dictation_bubble_background_color })
          }
        />
        <SettingSlider
          label="Bubble background opacity"
          description="Keep text and indicators visible even at 0%."
          value={settings.dictation_bubble_background_opacity}
          min={0}
          max={100}
          suffix="%"
          onValueChange={(dictation_bubble_background_opacity) =>
            void actions.onPatch({ dictation_bubble_background_opacity })
          }
        />
      </SettingsCard>
    </SettingsGrid>
  );
}
