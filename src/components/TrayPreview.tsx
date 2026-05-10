import { AudioWaveform, FolderOpen, Mic, Pause, Settings, Square, XCircle } from "lucide-react";
import { Badge, Button } from "./ui/primitives";
import type { AppSettings, AppStatus } from "../lib/types";

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
        <Badge tone={status.tray_state === "paused" ? "warning" : "primary"}>
          {status.tray_state}
        </Badge>
      </header>
      <div className="tray-menu">
        <button onClick={() => onPatch({ recording_enabled: !settings.recording_enabled })}>
          <Mic />
          Recording {settings.recording_enabled ? "On" : "Off"}
        </button>
        <button onClick={() => onPatch({ transcription_enabled: !settings.transcription_enabled })}>
          <AudioWaveform />
          Transcription {settings.transcription_enabled ? "On" : "Off"}
        </button>
        <button onClick={onRevealSaveFolder}>
          <FolderOpen />
          Reveal Save Folder
        </button>
        <button disabled>
          <Settings />
          Open Settings
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
