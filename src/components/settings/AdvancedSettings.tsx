import { CustomSourceSettings } from "./CustomSourceSettings";
import { SettingSwitch, SettingsCard, SettingsGrid } from "./settings-controls";
import { vadGateDisabledReason } from "./settings-helpers";
import type { AppSettings } from "@/lib/types";

export function AdvancedSettings({
  settings,
  onPatch,
}: {
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
}) {
  return (
    <SettingsGrid maxColumns={2}>
      <SettingsCard
        title="Text output"
        description="Send completed phrases to the active application."
      >
        <SettingSwitch
          label="Auto-type transcripts into cursor"
          checked={settings.auto_transcript_input_enabled}
          description="Paste each completed live transcript at the current cursor."
          onCheckedChange={(auto_transcript_input_enabled) =>
            onPatch({ auto_transcript_input_enabled })
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
            onPatch({ auto_transcript_input_trailing_space })
          }
        />
      </SettingsCard>

      <SettingsCard
        title="Transcript filtering"
        description="Expert transcript quality controls."
      >
        <SettingSwitch
          label="Hide low-confidence transcripts"
          checked={settings.suppress_low_confidence_transcripts}
          onCheckedChange={(suppress_low_confidence_transcripts) =>
            onPatch({ suppress_low_confidence_transcripts })
          }
        />
        <SettingSwitch
          label="VAD gate"
          description="Additional neural speech detection before transcription."
          checked={false}
          disabled
          title={vadGateDisabledReason()}
          onCheckedChange={() => {}}
        />
      </SettingsCard>

      <SettingsCard
        title="Custom system-audio sources"
        description="Window-title patterns and automatic capture behavior."
      >
        <CustomSourceSettings settings={settings} onPatch={onPatch} />
      </SettingsCard>
    </SettingsGrid>
  );
}
