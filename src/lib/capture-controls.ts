import type {
  AppSettings,
  AppStatus,
  MicrophoneDevice,
  MicrophonePriorityEntry,
} from "./types";

type PrioritySettings = Pick<
  AppSettings,
  "microphone_priority" | "selected_microphone" | "selected_microphone_label"
>;

export interface InputAvailability {
  canStart: boolean;
  activeLabel: string;
  warning: string | null;
  warningTone: "warning" | "danger" | null;
}

export function isLiveInputStreamErrored(
  status: Pick<AppStatus, "runtime_warning">,
): boolean {
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

export function inputAvailability(
  settings: PrioritySettings,
  microphones: readonly MicrophoneDevice[],
): InputAvailability {
  const priority = derivePriorityList(settings);
  const primary = priority[0];
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

  const nextPriority = priority
    .slice(1)
    .map((entry) => microphones.find((microphone) => microphone.id === entry.id))
    .find((microphone) => microphone?.available);
  if (nextPriority) {
    const primaryLabel = primary?.label || settings.selected_microphone_label;
    return {
      canStart: true,
      activeLabel: nextPriority.label,
      warning: `Primary input "${primaryLabel}" is unavailable. WakeNote will use "${nextPriority.label}".`,
      warningTone: "warning",
    };
  }

  const systemFallback = microphones.find(
    (microphone) => microphone.fallback && microphone.available,
  );
  if (systemFallback) {
    return {
      canStart: true,
      activeLabel: systemFallback.label,
      warning: `Selected inputs are unavailable. WakeNote will use "${systemFallback.label}" as the system fallback.`,
      warningTone: "warning",
    };
  }

  return {
    canStart: false,
    activeLabel: primary?.label || settings.selected_microphone_label || "No input",
    warning: "No available input device is selected.",
    warningTone: "danger",
  };
}
