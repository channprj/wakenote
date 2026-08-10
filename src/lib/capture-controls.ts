import type {
  AppSettings,
  AppStatus,
  CaptureMicrophoneEntry,
  MicrophoneDevice,
  MicrophonePriorityEntry,
} from "./types";

type PrioritySettings = Pick<
  AppSettings,
  "microphone_priority" | "selected_microphone" | "selected_microphone_label"
> &
  Partial<Pick<AppSettings, "capture_microphones">>;

export interface InputAvailability {
  canStart: boolean;
  activeLabel: string;
  warning: string | null;
  warningTone: "warning" | "danger" | null;
}

export function isLiveInputStreamErrored(
  status: Pick<AppStatus, "runtime_warning">,
): boolean {
  return Boolean(
    status.runtime_warning?.startsWith("Live input stream error:"),
  );
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

export function derivePriorityList(
  settings: PrioritySettings,
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

export function normalizeCaptureMicrophones(
  entries: readonly CaptureMicrophoneEntry[],
): CaptureMicrophoneEntry[] {
  const seen = new Set<string>();
  const normalized = entries
    .map((entry) => {
      const coreAudioUid = entry.core_audio_uid?.trim();
      return {
        id: entry.id.trim(),
        label:
          entry.label.trim() ||
          (entry.id === "default" ? "System Default" : entry.id.trim()),
        ...(coreAudioUid ? { core_audio_uid: coreAudioUid } : {}),
      };
    })
    .filter((entry) => {
      if (!entry.id || seen.has(entry.id)) {
        return false;
      }
      seen.add(entry.id);
      return true;
    })
    .slice(0, 2);
  if (
    normalized.some((entry) => entry.id === "default") &&
    normalized.length > 1
  ) {
    return normalized.slice(0, 1);
  }
  return normalized.length > 0
    ? normalized
    : [{ id: "default", label: "System Default" }];
}

export function rebindCaptureMicrophones(
  entries: readonly CaptureMicrophoneEntry[],
  microphones: readonly MicrophoneDevice[],
): CaptureMicrophoneEntry[] {
  const rebound = normalizeCaptureMicrophones(entries).map((entry) => {
    const exact = microphones.find(
      (microphone) => microphone.id === entry.id && microphone.available,
    );
    if (exact) return microphoneEntry(exact);

    const label = normalizedMicrophoneLabel(entry.label);
    const sameName = microphones.filter(
      (microphone) =>
        microphone.available &&
        normalizedMicrophoneLabel(microphone.label) === label,
    );
    return sameName.length === 1 ? microphoneEntry(sameName[0]) : entry;
  });

  return normalizeCaptureMicrophones(rebound);
}

function microphoneEntry(
  microphone: Pick<MicrophoneDevice, "id" | "label" | "core_audio_uid">,
): CaptureMicrophoneEntry {
  return {
    id: microphone.id,
    label: microphone.label,
    ...(microphone.core_audio_uid
      ? { core_audio_uid: microphone.core_audio_uid }
      : {}),
  };
}

function normalizedMicrophoneLabel(label: string) {
  return label
    .normalize("NFKC")
    .trim()
    .replace(/\s+/g, " ")
    .toLocaleLowerCase();
}

export function inputAvailability(
  settings: PrioritySettings,
  microphones: readonly MicrophoneDevice[],
): InputAvailability {
  const primary = rebindCaptureMicrophones(
    settings.capture_microphones ?? derivePriorityList(settings).slice(0, 1),
    microphones,
  )[0];
  const primaryDevice = microphones.find(
    (microphone) => microphone.id === primary?.id && microphone.available,
  );
  if (primaryDevice) {
    return {
      canStart: true,
      activeLabel: primaryDevice.label,
      warning: null,
      warningTone: null,
    };
  }

  return {
    canStart: false,
    activeLabel:
      primary?.label || settings.selected_microphone_label || "No input",
    warning: `Primary input "${primary?.label || settings.selected_microphone_label}" is unavailable. WakeNote will wait for the same device.`,
    warningTone: "danger",
  };
}
