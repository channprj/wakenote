import type { AppSettings, TranscriptionLanguage } from "@/lib/types";
import {
  SettingSelect,
  SettingSwitch,
  type SelectOption,
} from "./settings-controls";

export const TRANSLATION_LANGUAGES = [
  { value: "ko", label: "Korean" },
  { value: "en", label: "English" },
  { value: "ja", label: "Japanese" },
  { value: "zh", label: "Chinese" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
] as const satisfies ReadonlyArray<SelectOption<TranscriptionLanguage>>;

export function TranslationControls({
  scope,
  settings,
  onPatch,
}: {
  scope: "subtitle" | "transcription" | "dictation";
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
}) {
  const label = {
    subtitle: "subtitles",
    transcription: "transcripts",
    dictation: "dictation",
  }[scope];
  const enabled = `${scope}_translation_enabled` as const;
  const language = `${scope}_translation_language` as const;
  const target = settings[language];
  const hasTarget = target !== null && target !== "auto";
  return (
    <>
      <SettingSwitch
        label={`Translate ${label}`}
        checked={settings[enabled] && hasTarget}
        disabled={!hasTarget}
        description={
          hasTarget
            ? "Sends text to your OpenRouter model. Provider charges may apply. Original transcripts are preserved."
            : "Choose a translation language to enable translation. Original transcripts are preserved."
        }
        onCheckedChange={(value) => onPatch({ [enabled]: value })}
      />
      <SettingSelect
        label={`${label[0].toUpperCase()}${label.slice(1)} translation language`}
        value={hasTarget ? target : "none"}
        options={
          scope === "transcription"
            ? [{ value: "none", label: "Not set" }, ...TRANSLATION_LANGUAGES]
            : TRANSLATION_LANGUAGES
        }
        onValueChange={(value) => {
          if (scope === "transcription" && value === "none") {
            onPatch({
              transcription_translation_language: null,
              transcription_translation_enabled: false,
            });
          } else {
            onPatch({ [language]: value });
          }
        }}
      />
    </>
  );
}
