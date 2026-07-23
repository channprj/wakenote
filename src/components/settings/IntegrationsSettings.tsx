import { KeyRoundIcon, Trash2Icon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";
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
  const [apiKey, setApiKey] = useState("");

  return (
    <SettingsGrid maxColumns={3}>
      <SettingsCard title="Text output" description="Send completed phrases to the active application.">
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

      <SettingsCard title="Floating overlay" description="Live transcript surface above other windows.">
        <SettingSwitch
          label="Show floating overlay"
          checked={settings.show_floating_overlay}
          onCheckedChange={(show_floating_overlay) => actions.onPatch({ show_floating_overlay })}
        />
        <SettingSelect
          label="Floating overlay position"
          value={settings.floating_overlay_position}
          disabled={!settings.show_floating_overlay}
          title={
            settings.show_floating_overlay
              ? "Choose where the floating overlay appears."
              : "Enable the floating overlay before choosing its position."
          }
          options={[
            { value: "off", label: "Off" },
            { value: "top", label: "Top" },
            { value: "bottom", label: "Bottom" },
          ]}
          onValueChange={(floating_overlay_position) =>
            actions.onPatch({ floating_overlay_position })
          }
        />
        <SettingSlider
          label="Overlay font size"
          value={settings.floating_overlay_font_size_px}
          min={18}
          max={48}
          suffix=" px"
          onValueChange={(floating_overlay_font_size_px) =>
            actions.onPatch({ floating_overlay_font_size_px })
          }
        />
        <ColorSetting
          label="Overlay text color"
          value={settings.floating_overlay_text_color}
          onChange={(floating_overlay_text_color) =>
            actions.onPatch({ floating_overlay_text_color })
          }
        />
        <ColorSetting
          label="Overlay background color"
          value={settings.floating_overlay_background_color}
          onChange={(floating_overlay_background_color) =>
            actions.onPatch({ floating_overlay_background_color })
          }
        />
        <SettingSlider
          label="Overlay background opacity"
          value={settings.floating_overlay_background_opacity}
          min={0}
          max={100}
          suffix="%"
          onValueChange={(floating_overlay_background_opacity) =>
            actions.onPatch({ floating_overlay_background_opacity })
          }
        />
      </SettingsCard>

      <SettingsCard title="OpenRouter" description="Transcript summaries and detailed reports.">
        <Field orientation="responsive" className="settings-row">
          <FieldContent>
            <FieldLabel>API Key</FieldLabel>
            <FieldDescription>
              Stored in the system keychain, never in the settings file.
            </FieldDescription>
          </FieldContent>
          <div className="settings-inline-control">
            <Input
              type="password"
              autoComplete="off"
              value={apiKey}
              placeholder={
                snapshot.openrouter_key_configured
                  ? "Enter a new key to replace the saved key"
                  : "OpenRouter API key"
              }
              onChange={(event) => setApiKey(event.currentTarget.value)}
            />
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!apiKey.trim()}
              onClick={() => {
                actions.onSaveOpenRouterApiKey(apiKey);
                setApiKey("");
              }}
            >
              <KeyRoundIcon data-icon="inline-start" />
              Save
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={!snapshot.openrouter_key_configured}
              onClick={actions.onDeleteOpenRouterApiKey}
            >
              <Trash2Icon data-icon="inline-start" />
              Delete
            </Button>
          </div>
          <StatusBadge tone={snapshot.openrouter_key_configured ? "success" : "warning"}>
            {snapshot.openrouter_key_configured ? "API key saved" : "API key missing"}
          </StatusBadge>
        </Field>
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
          onValueChange={(llm_max_iterations) => actions.onPatch({ llm_max_iterations })}
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
      <FieldContent><FieldLabel>{label}</FieldLabel></FieldContent>
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
