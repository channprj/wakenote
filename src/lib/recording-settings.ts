import type { AppSettings, SettingsPatch } from "./types";

export type RecordingProfile = Pick<
  AppSettings,
  | "threshold_dbfs"
  | "attack_ms"
  | "release_ms"
  | "pre_roll_ms"
  | "lead_in_padding_ms"
  | "post_roll_ms"
  | "min_chunk_ms"
  | "max_chunk_ms"
>;

export const RECORDING_DEFAULTS: RecordingProfile = {
  threshold_dbfs: -40,
  attack_ms: 200,
  release_ms: 1_000,
  pre_roll_ms: 400,
  lead_in_padding_ms: 200,
  post_roll_ms: 400,
  min_chunk_ms: 800,
  max_chunk_ms: 180_000,
};

export const RECORDING_FIELD_HELP: Record<keyof RecordingProfile, string> = {
  threshold_dbfs:
    "Minimum input level treated as active sound. More negative values are more sensitive and may capture more background noise.",
  attack_ms:
    "Time the input must remain above Threshold before a chunk starts.",
  release_ms:
    "Time the input must remain below Threshold before chunk ending begins.",
  pre_roll_ms:
    "Real audio retained from immediately before activation and included at the start of the chunk.",
  lead_in_padding_ms:
    "Silence inserted at the beginning of each saved audio file before the captured audio.",
  post_roll_ms:
    "Additional audio retained after Release completes before the chunk is closed.",
  min_chunk_ms: "Minimum chunk duration before silence is allowed to close it.",
  max_chunk_ms:
    "Maximum chunk duration before WakeNote splits the recording into another chunk.",
};

export function resetRecordingSettingsPatch(): SettingsPatch {
  return {
    ...RECORDING_DEFAULTS,
    calibration_completed: false,
  };
}
