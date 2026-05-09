import { CheckCircle2, FolderOpen, Play, RefreshCw, Square } from "lucide-react";
import { LevelMeter } from "./LevelMeter";
import { ModelManager } from "./ModelManager";
import { QueuePanel } from "./QueuePanel";
import { TrayPreview } from "./TrayPreview";
import { Badge, Button, Section, Select, Slider, Switch, TextInput } from "./ui/primitives";
import { calibrationSettingsPatch } from "../lib/calibration";
import type { AppSnapshot, AppSettings } from "../lib/types";

const durationFields = [
  ["attack_ms", "Attack", 50, 2000],
  ["release_ms", "Release", 250, 5000],
  ["pre_roll_ms", "Pre-roll", 0, 1500],
  ["post_roll_ms", "Post-roll", 0, 2000],
  ["min_chunk_ms", "Min Chunk", 100, 5000],
  ["max_chunk_ms", "Max Chunk", 10000, 900000],
] as const;

export function SettingsPanel({
  activeSection,
  snapshot,
  onPatch,
  onRefresh,
  onStartLiveCapture,
  onStopLiveCapture,
  onChooseSaveRoot,
  onChooseModelDirectory,
  onRevealSaveFolder,
  onImportAudioFiles,
  onEnqueueBacklog,
  onCancelCurrent,
  onCancelCurrentOperation,
  onProcessNextTranscription,
  onRetry,
  onSkip,
  onVerifyModel,
  onDownloadModel,
  onCancelModelDownload,
  onDeleteModel,
}: {
  activeSection: string;
  snapshot: AppSnapshot;
  onPatch: (patch: Partial<AppSettings>) => void;
  onRefresh: () => void;
  onStartLiveCapture: () => void;
  onStopLiveCapture: () => void;
  onChooseSaveRoot: () => void;
  onChooseModelDirectory: () => void;
  onRevealSaveFolder: () => void;
  onImportAudioFiles: () => void;
  onEnqueueBacklog: () => void;
  onCancelCurrent: () => void;
  onCancelCurrentOperation: () => void;
  onProcessNextTranscription: () => void;
  onRetry: (id: number) => void;
  onSkip: (id: number) => void;
  onVerifyModel: (modelId: string) => void;
  onDownloadModel: (modelId: string) => void;
  onCancelModelDownload: (modelId: string) => void;
  onDeleteModel: (modelId: string) => void;
}) {
  const { settings, status, microphones, models, queue } = snapshot;
  const selectedMicrophone = microphones.find((mic) => mic.id === settings.selected_microphone);
  const selectedMicrophoneAvailable = selectedMicrophone?.available ?? false;
  const fallbackMicrophone = microphones.find((mic) => mic.fallback && mic.available);
  const canStartWithMicrophone = selectedMicrophoneAvailable || Boolean(fallbackMicrophone);
  const usableModelIds = new Set(
    models
      .filter((model) => ["ready", "installed", "unloaded"].includes(model.status))
      .map((model) => model.id),
  );
  const hasProcessablePendingJob = queue.jobs.some(
    (job) => job.status === "pending" && usableModelIds.has(job.model_id),
  );
  const liveInputStreamErrored = status.runtime_warning?.startsWith("Live input stream error:");
  const liveCaptureDisabled =
    settings.pause_all ||
    !settings.recording_enabled ||
    !canStartWithMicrophone ||
    (status.live_input_active && !liveInputStreamErrored);

  if (activeSection === "models") {
    return (
      <Section
        title="Models"
        eyebrow="Local runtime"
        actions={<Badge tone="primary">{settings.selected_model}</Badge>}
      >
        <ModelManager
          models={models}
          settings={settings}
          onPatch={onPatch}
          onVerify={onVerifyModel}
          onDownload={onDownloadModel}
          onCancelDownload={onCancelModelDownload}
          onDelete={onDeleteModel}
        />
      </Section>
    );
  }

  if (activeSection === "recording") {
    return (
      <Section title="Recording" eyebrow="Threshold gate">
        <div className="two-column">
          <LevelMeter
            settings={settings}
            status={status}
            onApplyThreshold={(threshold_dbfs) => onPatch(calibrationSettingsPatch(threshold_dbfs))}
          />
          <div className="control-grid">
            <Slider
              label="Threshold"
              value={settings.threshold_dbfs}
              min={-90}
              max={-10}
              suffix=" dBFS"
              onValueChange={(threshold_dbfs) => onPatch({ threshold_dbfs })}
            />
            {durationFields.map(([key, label, min, max]) => (
              <Slider
                key={key}
                label={label}
                value={settings[key]}
                min={min}
                max={max}
                step={key === "max_chunk_ms" ? 1000 : 50}
                suffix=" ms"
                onValueChange={(value) => onPatch({ [key]: value })}
              />
            ))}
          </div>
        </div>
      </Section>
    );
  }

  if (activeSection === "storage") {
    return (
      <Section
        title="Storage"
        eyebrow="Local archive"
        actions={
          <>
            {settings.save_root_confirmed ? (
              <Badge tone="success">Confirmed</Badge>
            ) : (
              <Button
                type="button"
                variant="secondary"
                disabled={settings.save_root.trim().length === 0}
                onClick={() => onPatch({ save_root: settings.save_root })}
              >
                <CheckCircle2 data-icon="inline-start" />
                Confirm Save Root
              </Button>
            )}
            <Button type="button" variant="secondary" onClick={onChooseSaveRoot}>
              <FolderOpen data-icon="inline-start" />
              Choose Folder
            </Button>
          </>
        }
      >
        <div className="control-grid">
          <TextInput
            label="Save Root"
            value={settings.save_root}
            onChange={(event) => onPatch({ save_root: event.currentTarget.value })}
          />
          <Select
            label="Audio Format"
            value={settings.audio_format}
            onChange={(event) =>
              onPatch({ audio_format: event.currentTarget.value as AppSettings["audio_format"] })
            }
          >
            <option value="m4a">M4A / AAC</option>
            <option value="wav">WAV / PCM</option>
          </Select>
          <div className="path-pattern">
            <span>Pattern</span>
            <code>{settings.save_root}/YYYYMMDD/HHMMSS.{settings.audio_format}</code>
            <code>{settings.save_root}/YYYYMMDD/HHMMSS.txt</code>
            <code>{settings.save_root}/YYYYMMDD/HHMMSS.json</code>
          </div>
        </div>
      </Section>
    );
  }

  if (activeSection === "history") {
    return (
      <Section title="History" eyebrow="Transcription queue">
        <QueuePanel
          queue={queue}
          canProcessTranscription={
            !settings.pause_all && settings.transcription_enabled && hasProcessablePendingJob
          }
          onImportAudioFiles={onImportAudioFiles}
          onEnqueueBacklog={onEnqueueBacklog}
          onCancelCurrent={onCancelCurrent}
          onProcessNext={onProcessNextTranscription}
          onRetry={onRetry}
          onSkip={onSkip}
        />
      </Section>
    );
  }

  if (activeSection === "privacy") {
    return (
      <Section title="Privacy" eyebrow="Offline defaults">
        <div className="settings-list">
          <ToggleRow
            label="Launch at login"
            checked={settings.launch_at_login}
            onChange={(launch_at_login) => onPatch({ launch_at_login })}
          />
          <ToggleRow label="VAD gate" checked={settings.vad_enabled} onChange={(vad_enabled) => onPatch({ vad_enabled })} />
          <div className="privacy-note">
            <Badge tone="success">Offline</Badge>
            <span>Local models keep audio and text on device.</span>
          </div>
        </div>
      </Section>
    );
  }

  if (activeSection === "advanced") {
    return (
      <Section title="Advanced" eyebrow="Runtime controls">
        <div className="settings-list">
          <ToggleRow
            label="Show tray icon"
            checked={settings.show_tray_icon}
            onChange={(show_tray_icon) => onPatch({ show_tray_icon })}
          />
          <ToggleRow
            label="Floating overlay"
            checked={settings.show_floating_overlay}
            onChange={(show_floating_overlay) => onPatch({ show_floating_overlay })}
          />
          <div className="model-directory-row">
            <TextInput
              label="Model Directory"
              value={settings.model_directory}
              onChange={(event) => onPatch({ model_directory: event.currentTarget.value })}
            />
            <Button type="button" variant="secondary" size="sm" onClick={onChooseModelDirectory}>
              <FolderOpen data-icon="inline-start" />
              Choose Folder
            </Button>
          </div>
          <TextInput
            label="Primary Color"
            value={settings.theme_primary_color}
            onChange={(event) => onPatch({ theme_primary_color: event.currentTarget.value })}
          />
        </div>
      </Section>
    );
  }

  if (activeSection === "about") {
    return (
      <Section title="About" eyebrow="Sagwan 0.1.0">
        <div className="about-grid">
          <div>
            <span>Stack</span>
            <strong>Tauri v2 · Rust · React · TypeScript</strong>
          </div>
          <div>
            <span>Primary</span>
            <strong>#0047AB</strong>
          </div>
          <div>
            <span>Active Model</span>
            <strong>{settings.selected_model}</strong>
          </div>
          <div>
            <span>Mode</span>
            <strong>{status.mode}</strong>
          </div>
        </div>
      </Section>
    );
  }

  return (
    <div className="general-grid">
      <Section
        title="General"
        eyebrow="Capture mode"
        actions={
          <>
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={onStartLiveCapture}
              disabled={liveCaptureDisabled}
            >
              <Play data-icon="inline-start" />
              Start Input
            </Button>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={onStopLiveCapture}
              disabled={!status.live_input_active}
            >
              <Square data-icon="inline-start" />
              Stop Input
            </Button>
            <Button type="button" variant="secondary" size="sm" onClick={onRefresh}>
              <RefreshCw data-icon="inline-start" />
              Refresh
            </Button>
          </>
        }
      >
        <div className="settings-list">
          <ToggleRow
            label="Recording"
            checked={settings.recording_enabled}
            onChange={(recording_enabled) => onPatch({ recording_enabled })}
          />
          <ToggleRow
            label="Transcription"
            checked={settings.transcription_enabled}
            onChange={(transcription_enabled) => onPatch({ transcription_enabled })}
          />
          <ToggleRow
            label="Pause all"
            checked={settings.pause_all}
            onChange={(pause_all) => onPatch({ pause_all })}
          />
          <Select
            label="Microphone"
            value={settings.selected_microphone}
            onChange={(event) => {
              const device = microphones.find((mic) => mic.id === event.currentTarget.value);
              onPatch({
                selected_microphone: event.currentTarget.value,
                selected_microphone_label: device?.label ?? event.currentTarget.value,
              });
            }}
          >
            {microphones.map((device) => (
              <option key={device.id} value={device.id} disabled={!device.available}>
                {device.label}
                {device.available ? "" : " (Unavailable)"}
              </option>
            ))}
          </Select>
          {!selectedMicrophoneAvailable ? (
            fallbackMicrophone ? (
              <div className="warning-banner">
                {settings.selected_microphone_label} is unavailable. Start Input will use{" "}
                {fallbackMicrophone.label}.
              </div>
            ) : (
              <div className="warning-banner">No available input device is selected.</div>
            )
          ) : null}
        </div>
      </Section>
      <TrayPreview
        settings={settings}
        status={status}
        onPatch={onPatch}
        onRevealSaveFolder={onRevealSaveFolder}
        onCancelCurrentOperation={onCancelCurrentOperation}
      />
      <LevelMeter
        settings={settings}
        status={status}
        onApplyThreshold={(threshold_dbfs) => onPatch(calibrationSettingsPatch(threshold_dbfs))}
      />
    </div>
  );
}

function ToggleRow({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <div className="toggle-row">
      <span>{label}</span>
      <Switch label={label} checked={checked} onCheckedChange={onChange} />
    </div>
  );
}
