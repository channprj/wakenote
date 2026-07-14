import type { TranscriptEntry } from "./transcript-log";

export const RECENT_TRANSCRIPT_LIMIT = 3;

export function newestTranscriptTextEntries(
  entries: readonly TranscriptEntry[],
): TranscriptEntry[] {
  const seen = new Set<string>();
  const visibleEntries: TranscriptEntry[] = [];

  for (const entry of [...entries].reverse()) {
    if (entry.status === "failed") {
      continue;
    }

    const text = entry.text.trim();
    if (!text) {
      continue;
    }

    const key = entry.audio_path ?? text;
    if (seen.has(key) || seen.has(text)) {
      continue;
    }

    seen.add(key);
    seen.add(text);
    visibleEntries.push(entry);
    if (visibleEntries.length === RECENT_TRANSCRIPT_LIMIT) {
      break;
    }
  }

  return visibleEntries.reverse();
}
