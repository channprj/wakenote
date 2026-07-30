import { CircleAlertIcon } from "lucide-react";
import { LevelMeter } from "@/components/LevelMeter";
import { SystemAudioSettings } from "@/components/SystemAudioSettings";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldLabel,
} from "@/components/ui/field";
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
import {
  inputAvailability,
  normalizeCaptureMicrophones,
} from "@/lib/capture-controls";
import {
  RECORDING_FIELD_HELP,
  resetRecordingSettingsPatch,
} from "@/lib/recording-settings";
import type {
  AppSettings,
  AppSnapshot,
  CaptureMicrophoneEntry,
  MicrophoneDevice,
} from "@/lib/types";
import { formatChunkDuration } from "./settings-helpers";
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
  const accessibilityPermission = snapshot.permissions.accessibility;
  const microphonePermission = snapshot.permissions.microphone;
  const screenRecordingPermission = snapshot.permissions.screen_recording;
  const availability = inputAvailability(settings, microphones);

  return (
    <SettingsGrid maxColumns={3}>
      <SettingsCard
        title="Permissions"
        description="Access required for live audio capture."
      >
        <PermissionSetting
          label="Accessibility Permission"
          status={accessibilityPermission.label}
          detail={accessibilityPermission.detail}
          actionLabel={
            accessibilityPermission.status === "granted"
              ? null
              : accessibilityPermission.can_request
                ? "Allow Accessibility"
                : accessibilityPermission.can_open_settings
                  ? "Open System Settings"
                  : null
          }
          onAction={actions.onRequestAccessibilityPermission}
        />
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
          onCheckedChange={(system_audio_enabled) =>
            actions.onPatch({ system_audio_enabled })
          }
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
        <SettingsCard
          title="Recognized system sources"
          description="Live detection and built-in source capture."
        >
          <SystemAudioSettings settings={settings} onPatch={actions.onPatch} />
        </SettingsCard>
      ) : null}

      <SettingsCard
        title="Microphone"
        description="Selection, monitoring, and language."
      >
        <MicrophoneSelectors
          value={settings.capture_microphones}
          microphones={microphones}
          onChange={(capture_microphones) =>
            actions.onPatch({ capture_microphones })
          }
        />
        <SettingSwitch
          label="Merge microphone inputs"
          description="Combine Primary and Secondary into one recording and transcription."
          checked={settings.merge_microphone_inputs}
          onCheckedChange={(merge_microphone_inputs) =>
            actions.onPatch({ merge_microphone_inputs })
          }
        />
        <MicrophoneStatusRows settings={settings} status={status} />
        {availability.warning ? (
          <Alert
            variant={
              availability.warningTone === "danger" ? "destructive" : "default"
            }
            data-tone={availability.warningTone}
            className="my-2"
          >
            <CircleAlertIcon />
            <AlertTitle>
              Input{" "}
              {availability.warningTone === "danger"
                ? "unavailable"
                : "fallback"}
            </AlertTitle>
            <AlertDescription>{availability.warning}</AlertDescription>
          </Alert>
        ) : null}
        <SettingSwitch
          label="Monitor input audio"
          description="Play the Primary microphone through the default output device."
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

      <SettingsCard
        title="Calibration"
        description="Live level and threshold calibration."
      >
        <LevelMeter
          settings={settings}
          status={status}
          onApplyThreshold={(threshold_dbfs) =>
            actions.onPatch(calibrationSettingsPatch(threshold_dbfs))
          }
          onResetRecordingSettings={() =>
            actions.onPatch(resetRecordingSettingsPatch())
          }
        />
        <SettingSlider
          label="Threshold"
          description={RECORDING_FIELD_HELP.threshold_dbfs}
          value={settings.threshold_dbfs}
          min={-90}
          max={-10}
          suffix=" dBFS"
          onValueChange={(threshold_dbfs) =>
            actions.onPatch({ threshold_dbfs })
          }
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

      <SettingsCard
        title="Chunk timing"
        description="Voice gate and recording boundaries."
      >
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

function MicrophoneStatusRows({
  settings,
  status,
}: {
  settings: AppSettings;
  status: AppSnapshot["status"];
}) {
  const selected = normalizeCaptureMicrophones(settings.capture_microphones);
  return (
    <div className="grid gap-2" aria-label="Microphone capture status">
      {selected.map((configured, index) => {
        const slot = index === 0 ? "primary" : "secondary";
        const runtime = status.microphone_captures.find(
          (item) => item.slot === slot,
        );
        const state = runtime?.reconnecting
          ? "Reconnecting"
          : runtime?.active
            ? "Active"
            : "Waiting";
        const level = runtime?.level.current_dbfs ?? -120;
        return (
          <div
            key={`${slot}:${configured.id}`}
            className="rounded-md border border-border/70 px-3 py-2"
            aria-label={`${slot === "primary" ? "Primary" : "Secondary"} microphone status`}
          >
            <div className="flex items-center justify-between gap-3">
              <span className="min-w-0 truncate text-sm font-medium">
                {slot === "primary" ? "Primary" : "Secondary"} ·{" "}
                {runtime?.label ?? configured.label}
              </span>
              <StatusBadge
                tone={
                  runtime?.active
                    ? "success"
                    : runtime?.warning
                      ? "warning"
                      : "neutral"
                }
              >
                {state}
              </StatusBadge>
            </div>
            <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
              <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-muted">
                <div
                  className="h-full bg-primary transition-[width]"
                  style={{
                    width: `${Math.max(0, Math.min(100, ((level + 90) / 90) * 100))}%`,
                  }}
                />
              </div>
              <span className="tabular-nums">{Math.round(level)} dBFS</span>
            </div>
            {runtime?.warning ? (
              <p className="mt-1 text-xs text-amber-700 dark:text-amber-300">
                {runtime.warning}
              </p>
            ) : null}
          </div>
        );
      })}
    </div>
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
        <StatusBadge
          tone={status.toLowerCase().includes("allow") ? "success" : "warning"}
        >
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

function MicrophoneSelectors({
  value,
  microphones,
  onChange,
}: {
  value: CaptureMicrophoneEntry[];
  microphones: MicrophoneDevice[];
  onChange: (next: CaptureMicrophoneEntry[]) => void;
}) {
  const selected = normalizeCaptureMicrophones(value);
  const primary = selected[0];
  const secondary = selected[1];
  const explicitDevices = microphones.filter(
    (device) => device.id !== "default",
  );
  const secondaryDisabled = primary.id === "default";
  const deviceEntry = (id: string) => {
    const device = microphones.find((microphone) => microphone.id === id);
    return device ? { id: device.id, label: device.label } : null;
  };

  return (
    <Field orientation="vertical" className="settings-row">
      <FieldContent>
        <FieldLabel>Capture microphones</FieldLabel>
        <FieldDescription>
          Choose one required Primary and one optional Secondary. Each
          microphone records and reconnects independently.
        </FieldDescription>
      </FieldContent>
      <div className="grid gap-3">
        <Select
          value={primary.id}
          onValueChange={(id) => {
            const nextPrimary = deviceEntry(id);
            if (!nextPrimary) return;
            onChange(
              normalizeCaptureMicrophones([
                nextPrimary,
                ...(id === "default" || secondary?.id === id || !secondary
                  ? []
                  : [secondary]),
              ]),
            );
          }}
        >
          <SelectTrigger
            size="sm"
            className="w-full"
            aria-label="Primary microphone"
          >
            <SelectValue placeholder="Select Primary microphone" />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              {microphones.map((device) => (
                <SelectItem key={device.id} value={device.id}>
                  {device.label}
                  {device.available ? "" : " (Unavailable)"}
                </SelectItem>
              ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <Select
          value={secondary?.id ?? "__none__"}
          disabled={secondaryDisabled}
          onValueChange={(id) => {
            if (id === "__none__") {
              onChange([primary]);
              return;
            }
            const nextSecondary = deviceEntry(id);
            if (nextSecondary)
              onChange(normalizeCaptureMicrophones([primary, nextSecondary]));
          }}
        >
          <SelectTrigger
            size="sm"
            className="w-full"
            aria-label="Secondary microphone"
          >
            <SelectValue
              placeholder={
                secondaryDisabled
                  ? "Choose a physical Primary first"
                  : "No Secondary microphone"
              }
            />
          </SelectTrigger>
          <SelectContent>
            <SelectGroup>
              <SelectItem value="__none__">No Secondary microphone</SelectItem>
              {explicitDevices
                .filter((device) => device.id !== primary.id)
                .map((device) => (
                  <SelectItem key={device.id} value={device.id}>
                    {device.label}
                    {device.available ? "" : " (Unavailable)"}
                  </SelectItem>
                ))}
            </SelectGroup>
          </SelectContent>
        </Select>
        <div className="flex items-center justify-between text-xs text-muted-foreground">
          <span>Input monitoring uses Primary only.</span>
          <StatusBadge tone={selected.length === 2 ? "success" : "neutral"}>
            {selected.length} / 2 selected
          </StatusBadge>
        </div>
      </div>
    </Field>
  );
}
