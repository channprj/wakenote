import {
  ArrowDownIcon,
  ArrowUpIcon,
  CircleAlertIcon,
  XIcon,
} from "lucide-react";
import { LevelMeter } from "@/components/LevelMeter";
import { SystemAudioSettings } from "@/components/SystemAudioSettings";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Field, FieldContent, FieldDescription, FieldLabel } from "@/components/ui/field";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { StatusBadge } from "@/components/ui/status-badge";
import { calibrationSettingsPatch } from "@/lib/calibration";
import { derivePriorityList, inputAvailability } from "@/lib/capture-controls";
import {
  RECORDING_FIELD_HELP,
  resetRecordingSettingsPatch,
} from "@/lib/recording-settings";
import type {
  AppSettings,
  AppSnapshot,
  MicrophoneDevice,
  MicrophonePriorityEntry,
} from "@/lib/types";
import {
  addMicrophonePriority,
  formatChunkDuration,
  removeMicrophonePriority,
  reorderMicrophonePriority,
} from "./settings-helpers";
import {
  SettingSelect,
  SettingSlider,
  SettingSwitch,
  SettingsCard,
  SettingsGrid,
} from "./settings-controls";
import type { SettingsActions } from "./types";

const durationFields = [
  ["attack_ms", "Attack", 50, 2_000],
  ["release_ms", "Release", 250, 5_000],
  ["pre_roll_ms", "Pre-roll", 0, 1_500],
  ["lead_in_padding_ms", "Lead-in", 0, 2_000],
  ["post_roll_ms", "Post-roll", 0, 2_000],
  ["min_chunk_ms", "Min Chunk", 100, 5_000],
] as const;

export function AudioSettings({
  snapshot,
  actions,
}: {
  snapshot: AppSnapshot;
  actions: SettingsActions;
}) {
  const { settings, status, microphones } = snapshot;
  const microphonePermission = snapshot.permissions.microphone;
  const screenRecordingPermission = snapshot.permissions.screen_recording;
  const availability = inputAvailability(settings, microphones);

  return (
    <SettingsGrid maxColumns={3}>
      <SettingsCard title="Permissions" description="Access required for live audio capture.">
        <PermissionSetting
          label="Microphone Permission"
          status={microphonePermission.label}
          detail={microphonePermission.detail}
          actionLabel={
            microphonePermission.status === "granted"
              ? null
              : microphonePermission.can_request
                ? "Allow Microphone"
                : microphonePermission.can_open_settings
                  ? "Open System Settings"
                  : null
          }
          onAction={actions.onRequestMicrophonePermission}
        />
        <SettingSwitch
          label="Capture system audio"
          description="Capture supported meeting and media applications."
          checked={settings.system_audio_enabled}
          onCheckedChange={(system_audio_enabled) => actions.onPatch({ system_audio_enabled })}
        />
        {settings.system_audio_enabled ? (
          <PermissionSetting
            label="Screen Recording Permission"
            status={screenRecordingPermission.label}
            detail={screenRecordingPermission.detail}
            actionLabel={
              screenRecordingPermission.status === "granted"
                ? null
                : screenRecordingPermission.can_request
                  ? "Allow Screen Recording"
                  : screenRecordingPermission.can_open_settings
                    ? "Open System Settings"
                    : null
            }
            onAction={actions.onRequestScreenRecordingPermission}
          />
        ) : null}
      </SettingsCard>

      {settings.system_audio_enabled ? (
        <SettingsCard title="Recognized system sources" description="Live detection and built-in source capture.">
          <SystemAudioSettings settings={settings} onPatch={actions.onPatch} />
        </SettingsCard>
      ) : null}

      <SettingsCard title="Microphone" description="Priority, monitoring, and language.">
        <MicrophonePriorityList
          value={derivePriorityList(settings)}
          microphones={microphones}
          onChange={(microphone_priority) => actions.onPatch({ microphone_priority })}
        />
        {availability.warning ? (
          <Alert
            variant={availability.warningTone === "danger" ? "destructive" : "default"}
            data-tone={availability.warningTone}
            className="my-2"
          >
            <CircleAlertIcon />
            <AlertTitle>Input {availability.warningTone === "danger" ? "unavailable" : "fallback"}</AlertTitle>
            <AlertDescription>{availability.warning}</AlertDescription>
          </Alert>
        ) : null}
        <SettingSwitch
          label="Monitor input audio"
          description="Play the selected microphone through the default output device."
          checked={settings.input_monitoring_enabled}
          onCheckedChange={(input_monitoring_enabled) =>
            actions.onPatch({ input_monitoring_enabled })
          }
        />
        <SettingSelect
          label="Transcription Language"
          value={settings.transcription_language}
          options={[
            { value: "auto", label: "Auto-detect" },
            { value: "ko", label: "Korean" },
            { value: "en", label: "English" },
            { value: "ja", label: "Japanese" },
            { value: "zh", label: "Chinese" },
            { value: "es", label: "Spanish" },
            { value: "fr", label: "French" },
            { value: "de", label: "German" },
          ]}
          onValueChange={(transcription_language) =>
            actions.onPatch({ transcription_language })
          }
        />
      </SettingsCard>

      <SettingsCard title="Calibration" description="Live level and threshold calibration.">
        <LevelMeter
          settings={settings}
          status={status}
          onApplyThreshold={(threshold_dbfs) =>
            actions.onPatch(calibrationSettingsPatch(threshold_dbfs))
          }
          onResetRecordingSettings={() => actions.onPatch(resetRecordingSettingsPatch())}
        />
        <SettingSlider
          label="Threshold"
          description={RECORDING_FIELD_HELP.threshold_dbfs}
          value={settings.threshold_dbfs}
          min={-90}
          max={-10}
          suffix=" dBFS"
          onValueChange={(threshold_dbfs) => actions.onPatch({ threshold_dbfs })}
        />
        <SettingSlider
          label="Mic Input Volume"
          description={RECORDING_FIELD_HELP.mic_input_volume_percent}
          value={settings.mic_input_volume_percent}
          min={0}
          max={200}
          step={5}
          suffix="%"
          onValueChange={(mic_input_volume_percent) =>
            actions.onPatch({ mic_input_volume_percent })
          }
        />
      </SettingsCard>

      <SettingsCard title="Chunk timing" description="Voice gate and recording boundaries.">
        {durationFields.map(([key, label, min, max]) => (
          <SettingSlider
            key={key}
            label={label}
            description={RECORDING_FIELD_HELP[key]}
            value={settings[key]}
            min={min}
            max={max}
            step={50}
            suffix=" ms"
            onValueChange={(value) => actions.onPatch({ [key]: value })}
          />
        ))}
        <SettingSlider
          label="Max Chunk"
          description={`${RECORDING_FIELD_HELP.max_chunk_ms} Current: ${formatChunkDuration(settings.max_chunk_ms)}.`}
          value={settings.max_chunk_ms}
          min={10_000}
          max={900_000}
          step={1_000}
          suffix=" ms"
          onValueChange={(max_chunk_ms) => actions.onPatch({ max_chunk_ms })}
        />
      </SettingsCard>
    </SettingsGrid>
  );
}

function PermissionSetting({
  label,
  status,
  detail,
  actionLabel,
  onAction,
}: {
  label: string;
  status: string;
  detail: string;
  actionLabel: string | null;
  onAction: () => void;
}) {
  return (
    <Field orientation="responsive" className="settings-row">
      <FieldContent>
        <FieldLabel>{label}</FieldLabel>
        <FieldDescription>{detail}</FieldDescription>
      </FieldContent>
      <div className="settings-action-row">
        <StatusBadge tone={status.toLowerCase().includes("allow") ? "success" : "warning"}>
          {status}
        </StatusBadge>
        {actionLabel ? (
          <Button type="button" size="sm" variant="outline" onClick={onAction}>
            {actionLabel}
          </Button>
        ) : null}
      </div>
    </Field>
  );
}

function MicrophonePriorityList({
  value,
  microphones,
  onChange,
}: {
  value: MicrophonePriorityEntry[];
  microphones: MicrophoneDevice[];
  onChange: (next: MicrophonePriorityEntry[]) => void;
}) {
  const availableToAdd = microphones.filter(
    (device) => !value.some((entry) => entry.id === device.id),
  );

  return (
    <Field orientation="vertical" className="settings-row">
      <FieldContent>
        <FieldLabel>Microphone priority</FieldLabel>
        <FieldDescription>
          WakeNote tries each device in order and returns to the primary input when available.
        </FieldDescription>
      </FieldContent>
      <ol className="settings-priority-list" aria-label="Microphone priority order">
        {value.map((entry, index) => {
          const availability = microphones.find((microphone) => microphone.id === entry.id)?.available;
          return (
            <li key={entry.id}>
              <span className="settings-priority-list__position">{index + 1}</span>
              <span className="min-w-0 truncate" title={`${entry.label} · ${entry.id}`}>
                {entry.label}
              </span>
              {index === 0 ? <StatusBadge tone="primary">Primary</StatusBadge> : null}
              {availability === false ? <StatusBadge tone="warning">Unavailable</StatusBadge> : null}
              {availability === undefined ? <StatusBadge tone="warning">Not connected</StatusBadge> : null}
              <div className="settings-priority-list__actions">
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Move ${entry.label} up`}
                  disabled={index === 0}
                  onClick={() => onChange(reorderMicrophonePriority(value, index, index - 1))}
                >
                  <ArrowUpIcon data-icon="solo" />
                </Button>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Move ${entry.label} down`}
                  disabled={index === value.length - 1}
                  onClick={() => onChange(reorderMicrophonePriority(value, index, index + 1))}
                >
                  <ArrowDownIcon data-icon="solo" />
                </Button>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`Remove ${entry.label}`}
                  disabled={value.length <= 1}
                  onClick={() => onChange(removeMicrophonePriority(value, index))}
                >
                  <XIcon data-icon="solo" />
                </Button>
              </div>
            </li>
          );
        })}
      </ol>
      {availableToAdd.length > 0 ? (
        <Select
          value=""
          onValueChange={(id) => {
            const device = microphones.find((microphone) => microphone.id === id);
            if (device) onChange(addMicrophonePriority(value, device));
          }}
        >
          <SelectTrigger size="sm" className="w-fit min-w-44" aria-label="Add microphone">
            <SelectValue placeholder="Add a microphone…" />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {availableToAdd.map((device) => (
                <SelectItem key={device.id} value={device.id}>
                  {device.label}{device.available ? "" : " (Unavailable)"}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
      ) : null}
    </Field>
  );
}
