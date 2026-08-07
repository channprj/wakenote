import { KeyRoundIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { StatusBadge } from "@/components/ui/status-badge";
import { Textarea } from "@/components/ui/textarea";
import type { AppSnapshot } from "@/lib/types";
import {
  SettingSelect,
  SettingSlider,
  SettingSwitch,
  SettingsCard,
  SettingsGrid,
} from "./settings-controls";
import type { SettingsActions } from "./types";

export function IntegrationsSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings } = snapshot;
  const [openRouterApiKey, setOpenRouterApiKey] = useState("");
  const [openAiApiKey, setOpenAiApiKey] = useState("");
  const [sonioxApiKey, setSonioxApiKey] = useState("");

  function updateSubtitle(
    patch: Parameters<SettingsActions["onPatch"]>[0],
    preview = true,
  ) {
    void Promise.resolve(actions.onPatch(patch)).then(() => {
      if (preview) {
        return actions.onPreviewSubtitle(patch);
      }
    });
  }

  return (
    <SettingsGrid maxColumns={3}>
      <SettingsCard
        title="Text output"
        description="Send completed phrases to the active application."
      >
        <SettingSwitch
          label="Auto-type transcripts into cursor"
          checked={settings.auto_transcript_input_enabled}
          description="Paste each completed live transcript at the current cursor."
          onCheckedChange={(auto_transcript_input_enabled) =>
            actions.onPatch({ auto_transcript_input_enabled })
          }
        />
        <SettingSwitch
          label="Add trailing space after auto-type"
          checked={settings.auto_transcript_input_trailing_space}
          disabled={!settings.auto_transcript_input_enabled}
          title={
            settings.auto_transcript_input_enabled
              ? "Append one space after each transcript."
              : "Enable auto-type before adding a trailing space."
          }
          onCheckedChange={(auto_transcript_input_trailing_space) =>
            actions.onPatch({ auto_transcript_input_trailing_space })
          }
        />
      </SettingsCard>

      <SettingsCard
        title="Subtitles"
        description="Show live speech as configurable subtitles above other windows."
      >
        <SettingSwitch
          label="Show subtitles"
          checked={settings.show_floating_overlay}
          onCheckedChange={(show_floating_overlay) =>
            updateSubtitle({ show_floating_overlay }, false)
          }
        />
        <SettingSelect
          label="Subtitle position"
          value={settings.floating_overlay_position}
          disabled={!settings.show_floating_overlay}
          title={
            settings.show_floating_overlay
              ? "Choose where subtitles appear."
              : "Enable subtitles before choosing their position."
          }
          options={[
            { value: "off", label: "Off" },
            { value: "top_left", label: "Top left" },
            { value: "top", label: "Top center" },
            { value: "top_right", label: "Top right" },
            { value: "bottom_left", label: "Bottom left" },
            { value: "bottom", label: "Bottom center" },
            { value: "bottom_right", label: "Bottom right" },
          ]}
          onValueChange={(floating_overlay_position) =>
            updateSubtitle({ floating_overlay_position })
          }
        />
        <SettingSlider
          label="Subtitle font size"
          description="A short example appears on screen while you adjust the size."
          value={settings.floating_overlay_font_size_px}
          min={10}
          max={48}
          suffix=" px"
          onValueChange={(floating_overlay_font_size_px) =>
            updateSubtitle({ floating_overlay_font_size_px })
          }
        />
        <ColorSetting
          label="Subtitle text color"
          value={settings.floating_overlay_text_color}
          onChange={(floating_overlay_text_color) =>
            updateSubtitle({ floating_overlay_text_color })
          }
        />
        <SettingSelect
          label="Subtitle background"
          value={settings.floating_overlay_background_color}
          options={[
            { value: "#000000", label: "Black" },
            { value: "#ffffff", label: "White" },
          ]}
          onValueChange={(floating_overlay_background_color) =>
            updateSubtitle({
              floating_overlay_background_color,
              floating_overlay_text_color:
                floating_overlay_background_color === "#ffffff"
                  ? "#000000"
                  : "#ffffff",
            })
          }
        />
        <SettingSlider
          label="Subtitle background opacity"
          value={settings.floating_overlay_background_opacity}
          min={0}
          max={100}
          suffix="%"
          onValueChange={(floating_overlay_background_opacity) =>
            updateSubtitle({ floating_overlay_background_opacity })
          }
        />
        <SettingSlider
          label="Subtitle duration"
          description="Keep each final subtitle visible from 1 to 10 seconds."
          value={settings.subtitle_duration_seconds}
          min={1}
          max={10}
          suffix=" sec"
          onValueChange={(subtitle_duration_seconds) =>
            updateSubtitle({ subtitle_duration_seconds }, false)
          }
        />
        <SettingSelect
          label="Subtitle animation"
          value={settings.subtitle_animation}
          options={[
            { value: "instant", label: "Instant" },
            { value: "fade", label: "Fade" },
            { value: "dissolve", label: "Dissolve" },
          ]}
          onValueChange={(subtitle_animation) =>
            updateSubtitle({ subtitle_animation })
          }
        />
      </SettingsCard>

      <SettingsCard
        title="External AI API keys"
        description="Manage cloud transcription and report credentials in one place. Keys stay in WakeNote's private app-data secret store."
      >
        <ApiCredentialRow
          provider="OpenRouter"
          description="Qwen3 ASR transcription and AI report generation."
          value={openRouterApiKey}
          configured={snapshot.openrouter_key_configured}
          onChange={setOpenRouterApiKey}
          onSave={() => {
            actions.onSaveOpenRouterApiKey(openRouterApiKey);
            setOpenRouterApiKey("");
          }}
          onDelete={actions.onDeleteOpenRouterApiKey}
        />
        <ApiCredentialRow
          provider="OpenAI"
          description="GPT Transcribe file and real-time speech recognition."
          value={openAiApiKey}
          configured={snapshot.openai_key_configured}
          onChange={setOpenAiApiKey}
          onSave={() => {
            actions.onSaveOpenAiApiKey(openAiApiKey);
            setOpenAiApiKey("");
          }}
          onDelete={actions.onDeleteOpenAiApiKey}
        />
        <ApiCredentialRow
          provider="Soniox"
          description="Async V5 and Real-time V5 speech recognition."
          value={sonioxApiKey}
          configured={snapshot.soniox_key_configured}
          onChange={setSonioxApiKey}
          onSave={() => {
            actions.onSaveSonioxApiKey(sonioxApiKey);
            setSonioxApiKey("");
          }}
          onDelete={actions.onDeleteSonioxApiKey}
        />
      </SettingsCard>

      <SettingsCard
        title="OpenRouter reports"
        description="Configure the model and prompts used for transcript summaries and detailed reports."
      >
        <Field orientation="responsive" className="settings-row">
          <FieldContent>
            <FieldLabel htmlFor="openrouter-model">OpenRouter Model</FieldLabel>
          </FieldContent>
          <Input
            id="openrouter-model"
            className="max-w-sm"
            value={settings.openrouter_model}
            onChange={(event) =>
              actions.onPatch({ openrouter_model: event.currentTarget.value })
            }
          />
        </Field>
        <SettingSlider
          label="Maximum agent iterations"
          value={settings.llm_max_iterations}
          min={1}
          max={30}
          onValueChange={(llm_max_iterations) =>
            actions.onPatch({ llm_max_iterations })
          }
        />
        <PromptSetting
          label="Summary Prompt Template"
          value={settings.llm_summary_prompt_template}
          rows={7}
          onChange={(llm_summary_prompt_template) =>
            actions.onPatch({ llm_summary_prompt_template })
          }
        />
        <PromptSetting
          label="Detailed Report Prompt Template"
          value={settings.llm_report_prompt_template}
          rows={9}
          onChange={(llm_report_prompt_template) =>
            actions.onPatch({ llm_report_prompt_template })
          }
        />
      </SettingsCard>
    </SettingsGrid>
  );
}

function ApiCredentialRow({
  provider,
  description,
  value,
  configured,
  onChange,
  onSave,
  onDelete,
}: {
  provider: "OpenRouter" | "OpenAI" | "Soniox";
  description: string;
  value: string;
  configured: boolean;
  onChange: (value: string) => void;
  onSave: () => void;
  onDelete: () => void;
}) {
  return (
    <Field orientation="responsive" className="settings-row">
      <FieldContent>
        <FieldLabel>{provider}</FieldLabel>
        <FieldDescription>{description}</FieldDescription>
      </FieldContent>
      <div className="settings-inline-control">
        <Input
          type="password"
          autoComplete="off"
          aria-label={`${provider} API Key`}
          value={value}
          placeholder={
            configured
              ? "Enter a new key to replace the saved key"
              : `${provider} API key`
          }
          onChange={(event) => onChange(event.currentTarget.value)}
        />
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={!value.trim()}
          onClick={onSave}
        >
          <KeyRoundIcon data-icon="inline-start" />
          Save
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={!configured}
          onClick={onDelete}
        >
          <Trash2Icon data-icon="inline-start" />
          Delete
        </Button>
      </div>
      <StatusBadge tone={configured ? "success" : "warning"}>
        {configured ? "API key saved" : "API key missing"}
      </StatusBadge>
    </Field>
  );
}

function ColorSetting({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (value: string) => void;
}) {
  return (
    <Field orientation="responsive" className="settings-row">
      <FieldContent>
        <FieldLabel>{label}</FieldLabel>
      </FieldContent>
      <Input
        aria-label={label}
        type="color"
        className="w-16 p-1"
        value={value}
        onChange={(event) => onChange(event.currentTarget.value)}
      />
    </Field>
  );
}

function PromptSetting({
  label,
  value,
  rows,
  onChange,
}: {
  label: string;
  value: string;
  rows: number;
  onChange: (value: string) => void;
}) {
  return (
    <Field orientation="vertical" className="settings-row">
      <FieldLabel>{label}</FieldLabel>
      <Textarea
        aria-label={label}
        value={value}
        rows={rows}
        className="max-h-64 resize-y font-mono text-xs"
        onChange={(event) => onChange(event.currentTarget.value)}
      />
    </Field>
  );
}
