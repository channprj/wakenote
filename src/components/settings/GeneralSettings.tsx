import type { AppSettings } from "@/lib/types";
import { Field, FieldContent, FieldLabel } from "@/components/ui/field";
import { Input } from "@/components/ui/input";
import { SettingSelect, SettingSwitch, SettingsCard, SettingsGrid } from "./settings-controls";

export function GeneralSettings({
  settings,
  onPatch,
}: {
  settings: AppSettings;
  onPatch: (patch: Partial<AppSettings>) => void;
}) {
  return (
    <SettingsGrid maxColumns={2}>
      <SettingsCard title="Capture behavior" description="Default recording lifecycle.">
        <SettingSwitch
          label="Recording"
          checked={settings.recording_enabled}
          onCheckedChange={(recording_enabled) => onPatch({ recording_enabled })}
        />
        <SettingSwitch
          label="Transcription"
          checked={settings.transcription_enabled}
          onCheckedChange={(transcription_enabled) => onPatch({ transcription_enabled })}
        />
        <SettingSwitch
          label="Pause all"
          description="Suspend capture and background transcription."
          checked={settings.pause_all}
          onCheckedChange={(pause_all) => onPatch({ pause_all })}
        />
        <SettingSwitch
          label="Start input on launch"
          checked={settings.start_live_input_on_launch}
          onCheckedChange={(start_live_input_on_launch) =>
            onPatch({ start_live_input_on_launch })
          }
        />
        <SettingSwitch
          label="Autoplay next transcript"
          description="Continue playback through the current transcript selection."
          checked={settings.autoplay_next_transcript}
          onCheckedChange={(autoplay_next_transcript) =>
            onPatch({ autoplay_next_transcript })
          }
        />
      </SettingsCard>

      <SettingsCard title="Application" description="Startup and window presentation.">
        <SettingSwitch
          label="Launch at login"
          checked={settings.launch_at_login}
          onCheckedChange={(launch_at_login) => onPatch({ launch_at_login })}
        />
        <SettingSelect
          label="Theme"
          value={settings.theme_mode}
          options={[
            { value: "dark", label: "Dark" },
            { value: "light", label: "Light" },
          ]}
          onValueChange={(theme_mode) => onPatch({ theme_mode })}
        />
        <Field orientation="responsive" className="settings-row">
          <FieldContent><FieldLabel htmlFor="theme-primary-color">Primary Color</FieldLabel></FieldContent>
          <Input
            id="theme-primary-color"
            value={settings.theme_primary_color}
            className="max-w-40"
            onChange={(event) => onPatch({ theme_primary_color: event.currentTarget.value })}
          />
        </Field>
        <SettingSwitch
          label="Show Dock icon"
          checked={settings.show_dock_icon}
          onCheckedChange={(show_dock_icon) => onPatch({ show_dock_icon })}
        />
        <SettingSwitch
          label="Show menu bar icon"
          checked={settings.show_tray_icon}
          onCheckedChange={(show_tray_icon) => onPatch({ show_tray_icon })}
        />
        <SettingSelect
          label="Menu bar icon left click"
          description="Right click uses the other action."
          value={settings.tray_left_click_action}
          options={[
            { value: "toggle_pause", label: "Toggle WakeNote on/off" },
            { value: "open_menu", label: "Open dropdown menu" },
          ]}
          onValueChange={(tray_left_click_action) => onPatch({ tray_left_click_action })}
        />
      </SettingsCard>
    </SettingsGrid>
  );
}
