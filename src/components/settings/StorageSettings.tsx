import { CheckCircle2Icon, FolderOpenIcon } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { StatusBadge } from "@/components/ui/status-badge";
import type { AppSettings } from "@/lib/types";
import { confirmSaveRootDisabledReason } from "./settings-helpers";
import { SettingSelect, SettingsCard, SettingsGrid } from "./settings-controls";
import type { SettingsActions } from "./types";

export function StorageSettings({
  settings,
  actions,
}: {
  settings: AppSettings;
  actions: SettingsActions;
}) {
  const saveRootReason = confirmSaveRootDisabledReason(settings);
  const pathPrefix = settings.save_root || "Choose a save folder";

  return (
    <SettingsGrid maxColumns={3}>
      <SettingsCard title="Save location" description="Local audio and transcript archive.">
        <Field orientation="vertical" className="settings-row">
          <FieldContent>
            <FieldLabel htmlFor="save-root">Save Root</FieldLabel>
            <FieldDescription className="overflow-wrap-anywhere">
              WakeNote creates one folder per local day.
            </FieldDescription>
          </FieldContent>
          <Input
            id="save-root"
            value={settings.save_root}
            title={settings.save_root || undefined}
            onChange={(event) => actions.onPatch({ save_root: event.currentTarget.value })}
          />
          <div className="settings-action-row">
            {settings.save_root_confirmed ? (
              <StatusBadge tone="success">Confirmed</StatusBadge>
            ) : (
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={saveRootReason !== null}
                title={saveRootReason ?? undefined}
                onClick={() => actions.onPatch({ save_root: settings.save_root })}
              >
                <CheckCircle2Icon data-icon="inline-start" />
                Confirm Save Root
              </Button>
            )}
            <Button type="button" size="sm" variant="outline" onClick={actions.onChooseSaveRoot}>
              <FolderOpenIcon data-icon="inline-start" />
              Choose Folder
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={saveRootReason !== null}
              title={saveRootReason ?? settings.save_root}
              onClick={actions.onRevealSaveFolder}
            >
              <FolderOpenIcon data-icon="inline-start" />
              Open Save Folder
            </Button>
          </div>
        </Field>
      </SettingsCard>

      <SettingsCard title="File format" description="Encoding used for new clips.">
        <SettingSelect
          label="Audio Format"
          value={settings.audio_format}
          options={[
            { value: "m4a", label: "M4A / AAC" },
            { value: "mp3", label: "MP3" },
            { value: "wav", label: "WAV / PCM" },
          ]}
          onValueChange={(audio_format) => actions.onPatch({ audio_format })}
        />
        <SettingSelect
          label="Compressed Bitrate"
          value={String(settings.audio_bitrate_kbps)}
          disabled={settings.audio_format === "wav"}
          title={settings.audio_format === "wav" ? "WAV uses uncompressed PCM" : undefined}
          options={[
            { value: "64", label: "64 kbps" },
            { value: "96", label: "96 kbps" },
            { value: "128", label: "128 kbps" },
          ]}
          onValueChange={(audio_bitrate_kbps) =>
            actions.onPatch({ audio_bitrate_kbps: Number(audio_bitrate_kbps) })
          }
        />
      </SettingsCard>

      <SettingsCard title="Path preview" description="Files created for each captured clip.">
        <div className="settings-path-list">
          <code
            title={`${pathPrefix}/YYYYMMDD/YYMMDD-HHMMSS.${settings.audio_format}`}
          >
            {pathPrefix}/YYYYMMDD/YYMMDD-HHMMSS.{settings.audio_format}
          </code>
          <code title={`${pathPrefix}/YYYYMMDD/YYMMDD-HHMMSS.txt`}>
            {pathPrefix}/YYYYMMDD/YYMMDD-HHMMSS.txt
          </code>
          <code title={`${pathPrefix}/YYYYMMDD/YYMMDD-HHMMSS.json`}>
            {pathPrefix}/YYYYMMDD/YYMMDD-HHMMSS.json
          </code>
        </div>
      </SettingsCard>
    </SettingsGrid>
  );
}
