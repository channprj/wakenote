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
import { defaultSettings } from "@/lib/app-state";
import type { AppSnapshot } from "@/lib/types";
import { SettingSlider, SettingsCard, SettingsGrid } from "./settings-controls";
import type { SettingsActions } from "./types";
import { SettingsTextEditor } from "./SettingsTextEditor";

export function IntegrationsSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings } = snapshot;
  const savePatch = actions.onSavePatch ?? actions.onPatch;
  const defaults = defaultSettings();
  const [openRouterApiKey, setOpenRouterApiKey] = useState("");
  const [openAiApiKey, setOpenAiApiKey] = useState("");
  const [sonioxApiKey, setSonioxApiKey] = useState("");

  return (
    <SettingsGrid maxColumns={3}>
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
        <SettingsTextEditor
          label="OpenRouter Model"
          multiline={false}
          value={settings.openrouter_model}
          defaultValue={defaults.openrouter_model}
          onSave={(openrouter_model) => savePatch({ openrouter_model })}
        />
        <SettingSlider
          label="Maximum agent iterations"
          value={settings.llm_max_iterations}
          min={1}
          max={30}
          onValueChange={(llm_max_iterations) =>
            actions.onPatch({ llm_max_iterations })
          }
        />
        <SettingsTextEditor
          label="Summary Prompt Template"
          description="Available variables: {{transcripts}}, {{date_range}}, {{selected_count}}. Save applies this prompt to new summaries."
          value={settings.llm_summary_prompt_template}
          defaultValue={defaults.llm_summary_prompt_template}
          rows={7}
          onSave={(llm_summary_prompt_template) =>
            savePatch({ llm_summary_prompt_template })
          }
        />
        <SettingsTextEditor
          label="Detailed Report Prompt Template"
          description="Available variables: {{transcripts}}, {{date_range}}, {{selected_count}}. Save applies this prompt to new reports."
          value={settings.llm_report_prompt_template}
          defaultValue={defaults.llm_report_prompt_template}
          rows={9}
          onSave={(llm_report_prompt_template) =>
            savePatch({ llm_report_prompt_template })
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
