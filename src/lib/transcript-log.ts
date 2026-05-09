/**
 * Transcript log state machine for the live-transcription footer.
 *
 * The backend fires five Tauri events per chunk lifecycle:
 *   started -> partial(s) -> committed -> final | failed
 * The footer wants to show one entry per chunk, transitioning through:
 *   listening -> partial -> queued (chunk closed, awaiting full pass) -> final
 * Final replaces partial text once the queue worker's accurate decode lands.
 */

export type TranscriptEntryStatus =
  | "listening"
  | "partial"
  | "queued"
  | "final"
  | "failed";

export interface TranscriptEntry {
  chunk_id: number;
  status: TranscriptEntryStatus;
  text: string;
  started_at: string;
  audio_path: string | null;
  error: string | null;
}

export type TranscriptEvent =
  | { type: "started"; chunk_id: number; started_at: string }
  | { type: "partial"; chunk_id: number; text: string }
  | { type: "committed"; chunk_id: number; audio_path: string }
  | {
      type: "final";
      chunk_id: number | null;
      audio_path: string;
      text: string;
    }
  | {
      type: "failed";
      chunk_id: number | null;
      audio_path: string;
      error: string;
    };

export const TRANSCRIPT_LOG_LIMIT = 12;

export function reduceTranscriptLog(
  entries: TranscriptEntry[],
  event: TranscriptEvent,
): TranscriptEntry[] {
  switch (event.type) {
    case "started":
      return appendOrReplace(entries, event.chunk_id, () => ({
        chunk_id: event.chunk_id,
        status: "listening",
        text: "",
        started_at: event.started_at,
        audio_path: null,
        error: null,
      }));

    case "partial":
      return updateEntry(entries, event.chunk_id, (entry) => {
        if (entry.status === "final" || entry.status === "failed") {
          return entry;
        }
        return { ...entry, status: "partial", text: event.text };
      });

    case "committed":
      return updateEntry(entries, event.chunk_id, (entry) => ({
        ...entry,
        status: entry.status === "final" ? entry.status : "queued",
        audio_path: event.audio_path,
      }));

    case "final":
      return updateOrAppendByAudio(entries, event.chunk_id, event.audio_path, (entry) => ({
        ...(entry ?? defaultEntry(event.chunk_id, event.audio_path)),
        chunk_id: event.chunk_id ?? entry?.chunk_id ?? -1,
        status: "final",
        text: event.text,
        audio_path: event.audio_path,
        error: null,
      }));

    case "failed":
      return updateOrAppendByAudio(entries, event.chunk_id, event.audio_path, (entry) => ({
        ...(entry ?? defaultEntry(event.chunk_id, event.audio_path)),
        chunk_id: event.chunk_id ?? entry?.chunk_id ?? -1,
        status: "failed",
        text: entry?.text ?? "",
        audio_path: event.audio_path,
        error: event.error,
      }));
  }
}

function defaultEntry(chunk_id: number | null, audio_path: string): TranscriptEntry {
  return {
    chunk_id: chunk_id ?? -1,
    status: "queued",
    text: "",
    started_at: new Date().toISOString(),
    audio_path,
    error: null,
  };
}

function appendOrReplace(
  entries: TranscriptEntry[],
  chunk_id: number,
  build: () => TranscriptEntry,
): TranscriptEntry[] {
  const existingIndex = entries.findIndex((entry) => entry.chunk_id === chunk_id);
  const nextEntry = build();
  const next = existingIndex >= 0
    ? entries.map((entry, index) => (index === existingIndex ? nextEntry : entry))
    : [...entries, nextEntry];
  return trimToLimit(next);
}

function updateEntry(
  entries: TranscriptEntry[],
  chunk_id: number,
  patch: (entry: TranscriptEntry) => TranscriptEntry,
): TranscriptEntry[] {
  if (!entries.some((entry) => entry.chunk_id === chunk_id)) {
    return entries;
  }
  return entries.map((entry) => (entry.chunk_id === chunk_id ? patch(entry) : entry));
}

function updateOrAppendByAudio(
  entries: TranscriptEntry[],
  chunk_id: number | null,
  audio_path: string,
  patch: (entry: TranscriptEntry | undefined) => TranscriptEntry,
): TranscriptEntry[] {
  let index = -1;
  if (chunk_id != null) {
    index = entries.findIndex((entry) => entry.chunk_id === chunk_id);
  }
  if (index < 0) {
    index = entries.findIndex((entry) => entry.audio_path === audio_path);
  }

  if (index >= 0) {
    const existing = entries[index];
    return entries.map((entry, i) => (i === index ? patch(existing) : entry));
  }

  return trimToLimit([...entries, patch(undefined)]);
}

function trimToLimit(entries: TranscriptEntry[]): TranscriptEntry[] {
  if (entries.length <= TRANSCRIPT_LOG_LIMIT) {
    return entries;
  }
  return entries.slice(entries.length - TRANSCRIPT_LOG_LIMIT);
}
