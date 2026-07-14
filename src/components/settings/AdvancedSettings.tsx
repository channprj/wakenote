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
    <SettingsGrid>
      <SettingsCard title="Transcript filtering" description="Expert transcript quality controls.">
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

      <SettingsCard title="Custom system-audio sources" description="Window-title patterns and automatic capture behavior.">
        <CustomSourceSettings settings={settings} onPatch={onPatch} />
      </SettingsCard>
    </SettingsGrid>
  );
}
