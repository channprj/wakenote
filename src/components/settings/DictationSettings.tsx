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
import { groupModelsByLocation, modelSupportsContext } from "@/lib/models";
import type {
  AppSettings,
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
import { DictionarySettingsCard } from "./DictionarySettingsCard";
import { RecentDictations } from "./RecentDictations";
import { SettingsTextEditor } from "./SettingsTextEditor";
import { TranslationControls } from "./TranslationControls";
import { defaultSettings } from "@/lib/app-state";
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
const DICTATION_HOTKEYS = [
  {
    field: "dictation_shortcut",
    label: "Hold-to-dictate hotkey",
    description:
      "Hold to record. Release to transcribe and type at the cursor.",
  },
  {
    field: "dictation_toggle_shortcut",
    label: "Toggle dictation hotkey",
    description:
      "Press once to record, then press again to transcribe and type. Leave unset to disable.",
  },
] as const;
type HotkeyField = (typeof DICTATION_HOTKEYS)[number]["field"];

export function dictationMicrophoneSummary(
  inputs: AppSettings["capture_microphones"],
) {
  const devices = inputs.slice(0, 2);
  return {
    label: devices.length > 1 ? "Automatic priority" : "Primary only",
    description:
      devices.map((device) => device.label).join(" + ") ||
      "No microphone selected",
  };
}

function isPhysicalModifierShortcut(shortcut: string): boolean {
  return /^(?:left|right)(?:ctrl|alt|shift|cmd)$/.test(shortcut);
}

function isModifierOnlyShortcut(shortcut: string): boolean {
  return (
    isPhysicalModifierShortcut(shortcut) ||
    shortcut
      .split("+")
      .every((part) => ["ctrl", "alt", "shift", "cmd"].includes(part))
  );
}

export function DictationSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings } = snapshot;
  const [capturingShortcut, setCapturingShortcut] =
    useState<HotkeyField | null>(null);
  const [shortcutError, setShortcutError] = useState<string | null>(null);
  const [errorField, setErrorField] = useState<HotkeyField | null>(null);
  const [shortcutBusy, setShortcutBusy] = useState(false);
  const mountedRef = useRef(true);
  const captureActiveRef = useRef<HotkeyField | null>(null);
  const shortcutBusyRef = useRef(false);
  const pendingModifierRef = useRef<string | null>(null);
  const shortcutButtons = useRef<
    Partial<Record<HotkeyField, HTMLButtonElement>>
  >({});

  useEffect(() => {
    if (capturingShortcut) shortcutButtons.current[capturingShortcut]?.focus();
  }, [capturingShortcut]);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      if (captureActiveRef.current) {
        captureActiveRef.current = null;
        void actions.onResumeDictationShortcut();
      }
    };
  }, [actions.onResumeDictationShortcut]);

  function setCaptureBusy(busy: boolean) {
    shortcutBusyRef.current = busy;
    if (mountedRef.current) setShortcutBusy(busy);
  }

  async function startShortcutCapture(field: HotkeyField) {
    if (
      !settings.dictation_enabled ||
      capturingShortcut ||
      captureActiveRef.current ||
      shortcutBusyRef.current
    ) {
      return;
    }
    setCaptureBusy(true);
    setErrorField(field);
    pendingModifierRef.current = null;
    setShortcutError(null);
    try {
      await actions.onSuspendDictationShortcut();
    } catch (error) {
      setShortcutError(error instanceof Error ? error.message : String(error));
      // A partially failed suspension may already have removed one hotkey.
      try {
        await actions.onResumeDictationShortcut();
      } catch (resumeError) {
        setShortcutError(`${String(error)}; ${String(resumeError)}`);
      }
      setCaptureBusy(false);
      return;
    }
    if (!mountedRef.current) {
      await actions.onResumeDictationShortcut();
      setCaptureBusy(false);
      return;
    }
    captureActiveRef.current = field;
    setCapturingShortcut(field);
    setCaptureBusy(false);
  }

  async function cancelShortcutCapture() {
    if (!captureActiveRef.current) return;
    captureActiveRef.current = null;
    setCapturingShortcut(null);
    setCaptureBusy(true);
    setShortcutError(null);
    try {
      await actions.onResumeDictationShortcut();
    } catch (error) {
      setShortcutError(error instanceof Error ? error.message : String(error));
    } finally {
      setCaptureBusy(false);
    }
  }

  async function commitShortcut(shortcut: string) {
    if (!captureActiveRef.current) {
      return;
    }
    const field = captureActiveRef.current;
    captureActiveRef.current = null;
    setCapturingShortcut(null);
    setCaptureBusy(true);
    setShortcutError(null);
    try {
      await (actions.onSavePatch ?? actions.onPatch)({
        [field]: shortcut,
      });
    } catch (error) {
      setShortcutError(error instanceof Error ? error.message : String(error));
    } finally {
      try {
        await actions.onResumeDictationShortcut();
      } catch (error) {
        setShortcutError(
          error instanceof Error ? error.message : String(error),
        );
      } finally {
        setCaptureBusy(false);
      }
    }
  }

  async function clearToggleShortcut() {
    if (shortcutBusyRef.current || captureActiveRef.current) return;
    setCaptureBusy(true);
    setErrorField("dictation_toggle_shortcut");
    setShortcutError(null);
    try {
      await (actions.onSavePatch ?? actions.onPatch)({
        dictation_toggle_shortcut: "",
      });
    } catch (error) {
      setShortcutError(error instanceof Error ? error.message : String(error));
    } finally {
      setCaptureBusy(false);
    }
  }

  useEffect(() => {
    if (!capturingShortcut) {
      return;
    }
    let cancelled = false;
    let polling = false;
    let nativeModifierSeen = false;
    const poll = async () => {
      if (polling) {
        return;
      }
      polling = true;
      try {
        const shortcut = await actions.onPressedModifierShortcut();
        if (!cancelled) {
          if (shortcut) {
            nativeModifierSeen = true;
            rememberModifierShortcut(shortcut);
          } else if (nativeModifierSeen && pendingModifierRef.current) {
            await commitShortcut(pendingModifierRef.current);
          }
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

  function rememberModifierShortcut(shortcut: string) {
    if (
      !pendingModifierRef.current ||
      shortcut.split("+").length >= pendingModifierRef.current.split("+").length
    ) {
      pendingModifierRef.current = shortcut;
    }
  }

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
    if (isModifierOnlyShortcut(shortcut)) {
      rememberModifierShortcut(shortcut);
      return;
    }

    await commitShortcut(shortcut);
  }

  async function captureModifierRelease(
    event: KeyboardEvent<HTMLButtonElement>,
  ) {
    if (!capturingShortcut || !captureActiveRef.current) {
      return;
    }
    const shortcut =
      pendingModifierRef.current ?? dictationShortcutFromKeyboardEvent(event);
    if (!shortcut || !isModifierOnlyShortcut(shortcut)) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    await commitShortcut(shortcut);
  }

  const dictationModels = snapshot.models.filter(
    (model) =>
      USABLE_MODEL_STATUSES.has(model.status) &&
      modelSupportsContext(model, "dictation"),
  );
  const dictationModelOptions: SelectOption<string>[] = [
    {
      value: DEFAULT_DICTATION_MODEL,
      label: "Default transcription model",
    },
    ...groupModelsByLocation(dictationModels).flatMap((group) =>
      group.models.map((model) => ({
        value: model.id,
        label: model.display_name,
        group: group.label,
      })),
    ),
  ];
  const microphoneSummary = dictationMicrophoneSummary(
    settings.capture_microphones,
  );

  return (
    <SettingsGrid maxColumns={2}>
      <SettingsCard
        title="Dictation hotkeys"
        description="Dictate into any app with hold or toggle recording. Press Escape to cancel."
      >
        <SettingSwitch
          label="Enable shortcut dictation"
          description="Disabled by default. WakeNote only listens for the shortcut when enabled."
          checked={settings.dictation_enabled}
          onCheckedChange={(dictation_enabled) =>
            void actions.onPatch({ dictation_enabled })
          }
        />
        {DICTATION_HOTKEYS.map(({ field, label, description }) => (
          <Field
            key={field}
            orientation="responsive"
            className="settings-row"
            data-disabled={!settings.dictation_enabled || undefined}
            data-invalid={
              (errorField === field && Boolean(shortcutError)) || undefined
            }
          >
            <FieldContent>
              <FieldLabel>{label}</FieldLabel>
              <FieldDescription>
                {errorField === field && shortcutError
                  ? shortcutError
                  : description}
              </FieldDescription>
            </FieldContent>
            <div className="flex max-w-full items-center gap-1.5">
              <Button
                type="button"
                variant="outline"
                size="sm"
                className="min-w-40 max-w-full"
                aria-label={label}
                ref={(element) => {
                  if (element) shortcutButtons.current[field] = element;
                }}
                aria-invalid={errorField === field && Boolean(shortcutError)}
                disabled={
                  !settings.dictation_enabled ||
                  shortcutBusy ||
                  (capturingShortcut !== null && capturingShortcut !== field)
                }
                onClick={() => void startShortcutCapture(field)}
                onKeyDown={(event) => void captureShortcut(event)}
                onKeyUp={(event) => void captureModifierRelease(event)}
                onBlur={() => {
                  if (captureActiveRef.current === field) {
                    void cancelShortcutCapture();
                  }
                }}
              >
                {capturingShortcut === field
                  ? "Press shortcut…"
                  : settings[field]
                    ? formatDictationShortcut(settings[field])
                    : "Not set"}
              </Button>
              {field === "dictation_toggle_shortcut" && settings[field] && (
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  aria-label="Clear toggle dictation hotkey"
                  disabled={
                    !settings.dictation_enabled ||
                    shortcutBusy ||
                    capturingShortcut !== null
                  }
                  onClick={() => void clearToggleShortcut()}
                >
                  Clear
                </Button>
              )}
            </div>
          </Field>
        ))}
        <p className="text-xs text-muted-foreground">
          Click a hotkey and press your keys. For a modifier chord or one
          physical modifier, release the keys to save.
        </p>
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
          description="Use the default transcription model or choose another compatible model only for Dictation."
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
        <Field orientation="responsive" className="settings-row">
          <FieldContent>
            <FieldLabel>Dictation microphones</FieldLabel>
            <FieldDescription>
              Uses the Primary and optional Secondary selected in Audio, then
              automatically keeps the cleanest signal.
            </FieldDescription>
          </FieldContent>
          <div
            aria-label="Dictation microphone mode"
            className="flex max-w-56 flex-col gap-0.5 @md/field-group:items-end @md/field-group:text-right"
          >
            <strong className="text-[length:var(--text-label)] leading-[var(--leading-label)] font-medium">
              {microphoneSummary.label}
            </strong>
            <span className="overflow-wrap-anywhere text-[length:var(--text-caption)] leading-[var(--leading-caption)] text-muted-foreground">
              {microphoneSummary.description}
            </span>
          </div>
        </Field>
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
      <RecentDictations
        snapshot={snapshot}
        onOpenHistory={actions.onOpenDictationHistory}
      />
      <DictionarySettingsCard
        settings={settings}
        status={snapshot.dictionary_file_status}
        onPatch={actions.onSavePatch ?? actions.onPatch}
        onOpenFile={actions.onOpenDictionaryFile}
        onReloadFile={actions.onReloadDictionaryFile}
      />
      <SettingsCard
        title="Translation & Enhanced Prompt"
        description="Keep normal Dictation as spoken text, or use a separate shortcut to turn your speech into a ready-to-use prompt."
      >
        <TranslationControls
          scope="dictation"
          settings={settings}
          onPatch={actions.onPatch}
        />
        <SettingSwitch
          label="Enhanced Prompt mode"
          checked={settings.enhance_prompt_enabled}
          description="Requires Dictation to be enabled and an OpenRouter key. Hold the dedicated shortcut to speak; release to format and type the prompt."
          onCheckedChange={(enhance_prompt_enabled) =>
            actions.onPatch({ enhance_prompt_enabled })
          }
        />
        <SettingsTextEditor
          label="Enhanced Prompt shortcut"
          multiline={false}
          description="Use a different key combination from Dictation, for example Ctrl+Alt+Space. Modifier-only shortcuts are not supported for this mode."
          value={settings.enhance_prompt_shortcut}
          defaultValue={defaultSettings().enhance_prompt_shortcut}
          onSave={(enhance_prompt_shortcut) =>
            (actions.onSavePatch ?? actions.onPatch)({
              enhance_prompt_shortcut,
            })
          }
        />
        <SettingsTextEditor
          label="Enhanced Prompt system prompt"
          description="Controls how spoken drafts are rewritten. The original recording and transcript stay in Transcripts."
          value={settings.enhance_prompt_system_prompt}
          defaultValue={defaultSettings().enhance_prompt_system_prompt}
          onSave={(enhance_prompt_system_prompt) =>
            (actions.onSavePatch ?? actions.onPatch)({
              enhance_prompt_system_prompt,
            })
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
