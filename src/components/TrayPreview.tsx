import { AudioWaveform, FolderOpen, Mic, Pause, Settings, Square, Type, XCircle } from "lucide-react";
import { humanizeTrayState } from "../lib/transcript-history";
import { trayStateBadgeTone } from "../lib/status-summary";
import { StatusBadge } from "./ui/status-badge";
import type { AppSettings, AppStatus } from "../lib/types";

export function openSettingsDisabledReason(): string {
  return "Settings panel is already open";
}

export function TrayPreview({
  settings,
  status,
  onPatch,
  onRevealSaveFolder,
  onCancelCurrentOperation,
}: {
  settings: AppSettings;
  status: AppStatus;
  onPatch: (patch: Partial<AppSettings>) => void;
  onRevealSaveFolder: () => void;
  onCancelCurrentOperation: () => void;
}) {
  return (
    <div className="tray-preview">
      <header>
        <div className="tray-preview__icon" data-state={status.tray_state}>
          <AudioWaveform />
        </div>
        <div>
          <strong>WakeNote</strong>
          <span>{status.active_microphone}</span>
        </div>
        <StatusBadge tone={trayStateBadgeTone(status.tray_state)}>
          {humanizeTrayState(status.tray_state)}
        </StatusBadge>
      </header>
      <div className="tray-menu">
        <button disabled title={openSettingsDisabledReason()}>
          <Settings />
          Open Settings
        </button>
        <span className="tray-menu__separator" role="separator" />
        <button onClick={() => onPatch({ recording_enabled: !settings.recording_enabled })}>
          <Mic />
          Recording {settings.recording_enabled ? "On" : "Off"}
        </button>
        <button onClick={() => onPatch({ transcription_enabled: !settings.transcription_enabled })}>
          <AudioWaveform />
          Transcription {settings.transcription_enabled ? "On" : "Off"}
        </button>
        <button onClick={() => onPatch({ auto_transcript_input_enabled: !settings.auto_transcript_input_enabled })}>
          <Type />
          Auto Type {settings.auto_transcript_input_enabled ? "On" : "Off"}
        </button>
        <button onClick={onRevealSaveFolder}>
          <FolderOpen />
          Reveal Save Folder
        </button>
        <button onClick={() => onPatch({ pause_all: !settings.pause_all })}>
          {settings.pause_all ? <Square /> : <Pause />}
          {settings.pause_all ? "Resume" : "Pause All"}
        </button>
        <button onClick={onCancelCurrentOperation}>
          <XCircle />
          Cancel Current Operation
        </button>
      </div>
      <footer>
        <span>{settings.selected_model}</span>
        <span>{settings.threshold_dbfs} dBFS</span>
      </footer>
    </div>
  );
}
