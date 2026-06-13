import { ArrowDown, ArrowUp, CheckCircle2, FolderOpen, Play, RefreshCw, Square, X } from "lucide-react";
import { LevelMeter } from "./LevelMeter";
import { ModelManager } from "./ModelManager";
import { QueuePanel } from "./QueuePanel";
import { TranscriptsPanel } from "./TranscriptsPanel";
import { AudioWorkspace } from "./AudioWorkspace";
import { Badge, Button, Section, Select, Slider, Switch, TextInput } from "./ui/primitives";
import { SystemAudioSettings } from "./SystemAudioSettings";
import { calibrationSettingsPatch, resetCalibrationSettingsPatch } from "../lib/calibration";
import { formatModelLabel } from "../lib/models";
import { modelStatusBadgeTone } from "../lib/status-summary";
import { fileUrlFromPath } from "../lib/transcript-history";
import type {
  AppSnapshot,
  AppSettings,
  AppStatus,
  MicrophoneDevice,
  MicrophonePriorityEntry,
} from "../lib/types";

const durationFields = [
  ["attack_ms", "Attack", 50, 2000],
  ["release_ms", "Release", 250, 5000],
  ["pre_roll_ms", "Pre-roll", 0, 1500],
  ["lead_in_padding_ms", "Lead-in", 0, 2000],
  ["post_roll_ms", "Post-roll", 0, 2000],
  ["min_chunk_ms", "Min Chunk", 100, 5000],
] as const;

const maxChunkPresets = [
  { label: "1 min", value: 60_000 },
  { label: "2 min", value: 120_000 },
  { label: "3 min", value: 180_000 },
  { label: "5 min", value: 300_000 },
] as const;

export function renderSaveRoot(saveRoot: string) {
  if (saveRoot.trim().length === 0) {
    return saveRoot;
  }
  return (
    <a href={fileUrlFromPath(saveRoot)} title={saveRoot}>
      {saveRoot}
    </a>
  );
}

function isLiveInputStreamErrored(status: Pick<AppStatus, "runtime_warning">): boolean {
  return Boolean(status.runtime_warning?.startsWith("Live input stream error:"));
}

export function startLiveCaptureDisabledReason(
  settings: Pick<AppSettings, "pause_all" | "recording_enabled">,
  status: Pick<AppStatus, "live_input_active" | "runtime_warning">,
  canStartWithMicrophone: boolean,
): string | null {
  if (status.live_input_active && !isLiveInputStreamErrored(status)) {
    return "Input is already running";
  }
  if (settings.pause_all) return "All capture is paused";
  if (!settings.recording_enabled) return "Recording is disabled";
  if (!canStartWithMicrophone) return "No microphone available";
  return null;
}

export function stopLiveCaptureDisabledReason(
  status: Pick<AppStatus, "live_input_active">,
): string | null {
  if (!status.live_input_active) return "Input is not running";
  return null;
}

export function confirmSaveRootDisabledReason(
  settings: Pick<AppSettings, "save_root">,
): string | null {
  if (settings.save_root.trim().length === 0) return "Enter a save folder first";
  return null;
}

export function vadGateDisabledReason(): string {
  return "Planned for v1 — Silero VAD will gate non-speech noise";
}

const transcriptionLanguageOptions: Array<{
  value: AppSettings["transcription_language"];
  label: string;
}> = [
  { value: "auto", label: "Auto-detect" },
  { value: "ko", label: "Korean" },
  { value: "en", label: "English" },
  { value: "ja", label: "Japanese" },
  { value: "zh", label: "Chinese" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
];

export function SettingsPanel({
  activeSection,
  snapshot,
  onPatch,
  onRefresh,
  onStartLiveCapture,
  onStopLiveCapture,
  onChooseSaveRoot,
  onRevealSaveFolder,
  onChooseModelDirectory,
  onRequestMicrophonePermission,
  onRequestScreenRecordingPermission,
  onImportAudioFiles,
  onEnqueueBacklog,
  onCancelCurrent,
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
  onRevealSaveFolder: () => void;
  onChooseModelDirectory: () => void;
  onRequestMicrophonePermission: () => void;
  onRequestScreenRecordingPermission: () => void;
  onImportAudioFiles: () => void;
  onEnqueueBacklog: () => void;
  onCancelCurrent: () => void;
  onProcessNextTranscription: () => void;
  onRetry: (id: number) => void;
  onSkip: (id: number) => void;
  onVerifyModel: (modelId: string) => void;
  onDownloadModel: (modelId: string) => void;
  onCancelModelDownload: (modelId: string) => void;
  onDeleteModel: (modelId: string) => void;
}) {
  const { settings, status, microphones, models, queue } = snapshot;
  const microphonePermission = snapshot.permissions.microphone;
  const screenRecordingPermission = snapshot.permissions.screen_recording;
  // The backend's invariant is priority[0].id === selected_microphone. When a
  // snapshot is constructed in tests or transient UI state, the two can drift;
  // we resync here so the UI always treats `selected_microphone` as position 0
  // and never crashes on missing fields.
  const priorityList = derivePriorityList(settings);
  const topPriorityEntry = priorityList[0];
  const topPriorityAvailable = microphones.some(
    (mic) => mic.id === topPriorityEntry.id && mic.available,
  );
  // Any entry below position 0 (or the system fallback) that the runtime can
  // actually open. As long as something usable is in the list, Start Input is
  // allowed — the watchdog handles the actual cycle.
  // The first available entry that sits below position 0 in priority — what
  // Start Input falls through to if the top mic is unreachable.
  const nextAvailablePriorityEntry = priorityList
    .slice(1)
    .find((entry) => microphones.some((mic) => mic.id === entry.id && mic.available));
  const fallbackMicrophone = microphones.find((mic) => mic.fallback && mic.available);
  const canStartWithMicrophone =
    topPriorityAvailable
    || Boolean(nextAvailablePriorityEntry)
    || Boolean(fallbackMicrophone);
  const usableModelIds = new Set(
    models
      .filter((model) => ["ready", "installed", "unloaded"].includes(model.status))
      .map((model) => model.id),
  );
  const hasProcessablePendingJob = queue.jobs.some(
    (job) => job.status === "pending" && usableModelIds.has(job.model_id),
  );
  const startLiveCaptureReason = startLiveCaptureDisabledReason(
    settings,
    status,
    canStartWithMicrophone,
  );
  const liveCaptureDisabled = startLiveCaptureReason !== null;
  const stopLiveCaptureReason = stopLiveCaptureDisabledReason(status);
  const confirmSaveRootReason = confirmSaveRootDisabledReason(settings);

  if (activeSection === "models") {
    return (
      <Section
        title="Models"
        eyebrow="Local runtime"
        actions={
          <Badge
            tone={modelStatusBadgeTone(
              models.find((model) => model.id === settings.selected_model)?.status ?? "",
            )}
          >
            {formatModelLabel(settings.selected_model, models)}
          </Badge>
        }
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
      <Section
        title="Recording"
        eyebrow="Threshold gate"
        actions={
          <>
            <Button
              type="button"
              variant="primary"
              size="sm"
              onClick={onStartLiveCapture}
              disabled={liveCaptureDisabled}
              title={startLiveCaptureReason ?? undefined}
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
              title={stopLiveCaptureReason ?? undefined}
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
        <div className="recording-stack">
          <div className="settings-list">
            <MicrophonePriorityList
              value={priorityList}
              microphones={microphones}
              onChange={(microphone_priority) => onPatch({ microphone_priority })}
            />
            {!topPriorityAvailable ? (
              nextAvailablePriorityEntry ? (
                <div className="warning-banner warning-banner--warning">
                  {topPriorityEntry.label} is unavailable. Start Input will use{" "}
                  {nextAvailablePriorityEntry.label}.
                </div>
              ) : fallbackMicrophone ? (
                <div className="warning-banner warning-banner--warning">
                  {topPriorityEntry.label} is unavailable. Start Input will use{" "}
                  {fallbackMicrophone.label}.
                </div>
              ) : (
                <div className="warning-banner warning-banner--danger">
                  No available input device is selected.
                </div>
              )
            ) : null}
          </div>
          <div className="recording-split">
            <LevelMeter
              settings={settings}
              status={status}
              onApplyThreshold={(threshold_dbfs) =>
                onPatch(calibrationSettingsPatch(threshold_dbfs))
              }
              onResetThreshold={() => onPatch(resetCalibrationSettingsPatch())}
            />
            <div className="control-grid control-grid--stacked">
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
                  step={50}
                  suffix=" ms"
                  onValueChange={(value) => onPatch({ [key]: value })}
                />
              ))}
              <MaxChunkControl
                value={settings.max_chunk_ms}
                onChange={(max_chunk_ms) => onPatch({ max_chunk_ms })}
              />
            </div>
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
                title={confirmSaveRootReason ?? undefined}
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
            <Button
              type="button"
              variant="secondary"
              disabled={settings.save_root.trim().length === 0}
              title={
                settings.save_root.trim().length === 0
                  ? "Enter a save folder first"
                  : settings.save_root
              }
              onClick={onRevealSaveFolder}
            >
              <FolderOpen data-icon="inline-start" />
              Open Save Folder
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
            <option value="mp3">MP3</option>
            <option value="wav">WAV / PCM</option>
          </Select>
          <Select
            label="Compressed Bitrate"
            value={String(settings.audio_bitrate_kbps)}
            disabled={settings.audio_format === "wav"}
            title={settings.audio_format === "wav" ? "WAV uses uncompressed PCM" : undefined}
            onChange={(event) =>
              onPatch({ audio_bitrate_kbps: Number(event.currentTarget.value) })
            }
          >
            <option value="64">64 kbps</option>
            <option value="96">96 kbps</option>
            <option value="128">128 kbps</option>
          </Select>
          <div className="path-pattern">
            <span>Pattern</span>
            <code>{renderSaveRoot(settings.save_root)}/YYYYMMDD/HHMMSS.{settings.audio_format}</code>
            <code>{renderSaveRoot(settings.save_root)}/YYYYMMDD/HHMMSS.txt</code>
            <code>{renderSaveRoot(settings.save_root)}/YYYYMMDD/HHMMSS.json</code>
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
          models={models}
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

  if (activeSection === "audio") {
    return (
      <Section title="Audio" eyebrow="Upload & meeting transcription">
        <AudioWorkspace />
      </Section>
    );
  }

  if (activeSection === "transcripts") {
    return (
      <Section title="Transcripts" eyebrow="Daily archive">
        <TranscriptsPanel
          customSources={settings.custom_sources}
          models={models}
          selectedModelId={settings.selected_model}
        />
      </Section>
    );
  }

  if (activeSection === "advanced") {
    return (
      <Section title="Advanced" eyebrow="Runtime controls">
        <div className="settings-list">
          <ToggleRow
            label="Show Dock icon"
            checked={settings.show_dock_icon}
            onChange={(show_dock_icon) => onPatch({ show_dock_icon })}
          />
          <ToggleRow
            label="Show menu bar icon"
            checked={settings.show_tray_icon}
            onChange={(show_tray_icon) => onPatch({ show_tray_icon })}
          />
          <Select
            label="Menu bar icon left click"
            value={settings.tray_left_click_action}
            onChange={(event) =>
              onPatch({
                tray_left_click_action: event.currentTarget
                  .value as AppSettings["tray_left_click_action"],
              })
            }
          >
            <option value="toggle_pause">Toggle WakeNote on/off</option>
            <option value="open_menu">Open dropdown menu</option>
          </Select>
          <p className="settings-hint">Right click uses the other action.</p>
          <Select
            label="Floating overlay"
            value={settings.floating_overlay_position}
            onChange={(event) =>
              onPatch({
                floating_overlay_position: event.currentTarget
                  .value as AppSettings["floating_overlay_position"],
              })
            }
          >
            <option value="off">Off</option>
            <option value="top">Top</option>
            <option value="bottom">Bottom</option>
          </Select>
          <Select
            label="Theme"
            value={settings.theme_mode}
            onChange={(event) =>
              onPatch({ theme_mode: event.currentTarget.value as AppSettings["theme_mode"] })
            }
          >
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </Select>
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

  return (
    <Section title="General" eyebrow="Application defaults">
      <div className="settings-list">
        <PermissionRow
          label="Microphone Permission"
          statusLabel={microphonePermission.label}
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
          onAction={onRequestMicrophonePermission}
        />
        <ToggleRow
          label="Capture system audio"
          checked={settings.system_audio_enabled}
          onChange={(system_audio_enabled) => onPatch({ system_audio_enabled })}
          title="Capture Google Meet, Zoom, YouTube, and custom app audio via ScreenCaptureKit (macOS 13+). Requires Screen Recording permission."
        />
        {settings.system_audio_enabled ? (
          <>
            <PermissionRow
              label="Screen Recording Permission"
              statusLabel={screenRecordingPermission.label}
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
              onAction={onRequestScreenRecordingPermission}
            />
            <SystemAudioSettings settings={settings} onPatch={onPatch} />
          </>
        ) : null}
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
          label="Start input on launch"
          checked={settings.start_live_input_on_launch}
          onChange={(start_live_input_on_launch) => onPatch({ start_live_input_on_launch })}
        />
        <ToggleRow
          label="Auto-type transcripts into cursor"
          checked={settings.auto_transcript_input_enabled}
          onChange={(auto_transcript_input_enabled) =>
            onPatch({ auto_transcript_input_enabled })
          }
          title="When a live transcript finishes, paste it into the currently focused text cursor."
        />
        <ToggleRow
          label="Add trailing space after auto-type"
          checked={settings.auto_transcript_input_trailing_space}
          disabled={!settings.auto_transcript_input_enabled}
          onChange={(auto_transcript_input_trailing_space) =>
            onPatch({ auto_transcript_input_trailing_space })
          }
          title={
            settings.auto_transcript_input_enabled
              ? "Append one space after each auto-typed transcript."
              : "Enable auto-type before adding a trailing space."
          }
        />
        <ToggleRow
          label="Hide low-confidence transcripts"
          checked={settings.suppress_low_confidence_transcripts}
          onChange={(suppress_low_confidence_transcripts) =>
            onPatch({ suppress_low_confidence_transcripts })
          }
        />
        <ToggleRow
          label="Launch at login"
          checked={settings.launch_at_login}
          onChange={(launch_at_login) => onPatch({ launch_at_login })}
        />
        <ToggleRow
          label="VAD gate"
          checked={false}
          disabled
          onChange={() => {}}
          title={vadGateDisabledReason()}
        />
        <Select
          label="Transcription Language"
          value={settings.transcription_language}
          onChange={(event) =>
            onPatch({
              transcription_language: event.currentTarget
                .value as AppSettings["transcription_language"],
            })
          }
        >
          {transcriptionLanguageOptions.map((option) => (
            <option key={option.value} value={option.value}>
              {option.label}
            </option>
          ))}
        </Select>
        <ToggleRow
          label="Pause all"
          checked={settings.pause_all}
          onChange={(pause_all) => onPatch({ pause_all })}
        />
        <div className="offline-note">
          <Badge tone="success">Offline</Badge>
          <span>Local models keep audio and text on device.</span>
        </div>
      </div>
    </Section>
  );
}

function PermissionRow({
  label,
  statusLabel,
  detail,
  actionLabel,
  onAction,
}: {
  label: string;
  statusLabel: string;
  detail: string;
  actionLabel: string | null;
  onAction: () => void;
}) {
  return (
    <div className="permission-row">
      <div>
        <span>{label}</span>
        <strong>{statusLabel}</strong>
        <p>{detail}</p>
      </div>
      {actionLabel ? (
        <Button type="button" variant="secondary" size="sm" onClick={onAction}>
          {actionLabel}
        </Button>
      ) : null}
    </div>
  );
}

function ToggleRow({
  label,
  checked,
  onChange,
  disabled = false,
  title,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  title?: string;
}) {
  return (
    <div className="toggle-row">
      <span>{label}</span>
      <Switch
        label={label}
        checked={checked}
        disabled={disabled}
        onCheckedChange={onChange}
        title={title}
      />
    </div>
  );
}

function MaxChunkControl({
  value,
  onChange,
}: {
  value: number;
  onChange: (value: number) => void;
}) {
  return (
    <div className="max-chunk-control">
      <Slider
        label="Max Chunk"
        value={value}
        min={10_000}
        max={900_000}
        step={1_000}
        suffix={` ms (${formatChunkDuration(value)})`}
        onValueChange={onChange}
      />
      <div className="max-chunk-presets" aria-label="Max chunk presets">
        {maxChunkPresets.map((preset) => (
          <Button
            key={preset.value}
            type="button"
            variant="secondary"
            size="sm"
            aria-pressed={value === preset.value}
            onClick={() => onChange(preset.value)}
          >
            {preset.label}
          </Button>
        ))}
      </div>
    </div>
  );
}

function formatChunkDuration(value: number) {
  if (value % 60_000 === 0) {
    return `${value / 60_000} min`;
  }

  return `${Math.round(value / 1_000)} sec`;
}

export function derivePriorityList(
  settings: Pick<
    AppSettings,
    "microphone_priority" | "selected_microphone" | "selected_microphone_label"
  >,
): MicrophonePriorityEntry[] {
  const fromBackend = settings.microphone_priority ?? [];
  const legacyTop: MicrophonePriorityEntry = {
    id: settings.selected_microphone,
    label: settings.selected_microphone_label,
  };
  if (fromBackend.length === 0) {
    return [legacyTop];
  }
  if (fromBackend[0]?.id === legacyTop.id) {
    return fromBackend;
  }
  return [
    legacyTop,
    ...fromBackend.filter((entry) => entry.id !== legacyTop.id),
  ];
}

export function reorderMicrophonePriority(
  list: MicrophonePriorityEntry[],
  from: number,
  to: number,
): MicrophonePriorityEntry[] {
  if (from === to || from < 0 || from >= list.length || to < 0 || to >= list.length) {
    return list;
  }
  const next = list.slice();
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

export function addMicrophonePriority(
  list: MicrophonePriorityEntry[],
  device: Pick<MicrophoneDevice, "id" | "label">,
): MicrophonePriorityEntry[] {
  if (list.some((entry) => entry.id === device.id)) {
    return list;
  }
  return [...list, { id: device.id, label: device.label }];
}

export function removeMicrophonePriority(
  list: MicrophonePriorityEntry[],
  index: number,
): MicrophonePriorityEntry[] {
  if (list.length <= 1 || index < 0 || index >= list.length) {
    return list;
  }
  return list.filter((_, i) => i !== index);
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
  const availabilityFor = (id: string): boolean | undefined => {
    const match = microphones.find((mic) => mic.id === id);
    return match ? match.available : undefined;
  };

  return (
    <div className="mic-priority">
      <span className="ui-field__label">Microphone priority</span>
      <ol className="mic-priority__list" aria-label="Microphone priority order">
        {value.map((entry, index) => {
          const available = availabilityFor(entry.id);
          const unknown = available === undefined;
          const isTop = index === 0;
          return (
            <li className="mic-priority__item" key={entry.id}>
              <span className="mic-priority__pos">{index + 1}.</span>
              <span className="mic-priority__label" title={entry.id}>
                <span className="mic-priority__name">{entry.label}</span>
                {isTop ? <Badge tone="primary">Primary</Badge> : null}
                {unknown ? (
                  <Badge tone="warning">Not connected</Badge>
                ) : available ? null : (
                  <Badge tone="warning">Unavailable</Badge>
                )}
              </span>
              <div className="mic-priority__controls">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label={`Move ${entry.label} up`}
                  disabled={index === 0}
                  onClick={() => onChange(reorderMicrophonePriority(value, index, index - 1))}
                >
                  <ArrowUp data-icon="solo" />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label={`Move ${entry.label} down`}
                  disabled={index === value.length - 1}
                  onClick={() => onChange(reorderMicrophonePriority(value, index, index + 1))}
                >
                  <ArrowDown data-icon="solo" />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  aria-label={`Remove ${entry.label}`}
                  disabled={value.length <= 1}
                  onClick={() => onChange(removeMicrophonePriority(value, index))}
                >
                  <X data-icon="solo" />
                </Button>
              </div>
            </li>
          );
        })}
      </ol>
      {availableToAdd.length > 0 ? (
        <Select
          label="Add microphone"
          value=""
          onChange={(event) => {
            const id = event.currentTarget.value;
            if (!id) return;
            const device = microphones.find((mic) => mic.id === id);
            if (device) {
              onChange(addMicrophonePriority(value, device));
            }
            event.currentTarget.value = "";
          }}
        >
          <option value="">Add a microphone…</option>
          {availableToAdd.map((device) => (
            <option key={device.id} value={device.id}>
              {device.label}
              {device.available ? "" : " (Unavailable)"}
            </option>
          ))}
        </Select>
      ) : null}
      <p className="mic-priority__hint">
        Capture opens position 1 first. If it stalls, the watchdog cycles through the list in
        order and retries position 1 every 10 minutes.
      </p>
    </div>
  );
}
