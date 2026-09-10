import type { TranscriptionLanguage } from "./types";

export const DEFAULT_TRANSCRIPTION_LANGUAGE_HINTS: TranscriptionLanguage[] = [
  "en",
  "ko",
];

export const TRANSCRIPTION_LANGUAGE_OPTIONS = [
  { value: "en", label: "English" },
  { value: "ko", label: "Korean" },
  { value: "ja", label: "Japanese" },
  { value: "zh", label: "Chinese" },
  { value: "es", label: "Spanish" },
  { value: "fr", label: "French" },
  { value: "de", label: "German" },
] as const;
