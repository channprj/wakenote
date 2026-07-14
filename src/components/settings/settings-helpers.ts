import type {
  AppSettings,
  MicrophoneDevice,
  MicrophonePriorityEntry,
} from "@/lib/types";

export function confirmSaveRootDisabledReason(
  settings: Pick<AppSettings, "save_root">,
): string | null {
  return settings.save_root.trim() ? null : "Enter a save folder first";
}

export function vadGateDisabledReason(): string {
  return "Planned for v1 — Silero VAD will gate non-speech noise";
}

export function formatChunkDuration(value: number): string {
  return value % 60_000 === 0
    ? `${value / 60_000} min`
    : `${Math.round(value / 1_000)} sec`;
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
  return list.filter((_, currentIndex) => currentIndex !== index);
}
