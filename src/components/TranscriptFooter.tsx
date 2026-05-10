import { AlertCircle, AudioLines, CheckCircle2, Loader2 } from "lucide-react";
import { useEffect, useMemo, useRef, type ReactElement } from "react";
import type { TranscriptEntry, TranscriptEntryStatus } from "../lib/transcript-log";

const statusLabel: Record<TranscriptEntryStatus, string> = {
  listening: "Listening",
  partial: "Live",
  queued: "Refining",
  final: "Saved",
  failed: "Review",
};

const statusIcon: Record<TranscriptEntryStatus, ReactElement> = {
  listening: <AudioLines aria-hidden />,
  partial: <Loader2 aria-hidden className="transcript-footer__spin" />,
  queued: <Loader2 aria-hidden className="transcript-footer__spin" />,
  final: <CheckCircle2 aria-hidden />,
  failed: <AlertCircle aria-hidden />,
};

const RECENT_TRANSCRIPT_LIMIT = 3;

export function TranscriptFooter({
  entries,
  liveActive,
}: {
  entries: TranscriptEntry[];
  liveActive: boolean;
}) {
  const visibleEntries = useMemo(() => newestTranscriptTextEntries(entries), [entries]);
  const listRef = useRef<HTMLOListElement>(null);
  const scrollKey = visibleEntries
    .map((entry) => `${entry.chunk_id}:${entry.status}:${entry.audio_path ?? ""}:${entry.text}`)
    .join("\n");

  useEffect(() => {
    scrollTranscriptListToBottom(listRef.current);
  }, [scrollKey, liveActive]);

  if (visibleEntries.length === 0 && !liveActive) {
    return null;
  }

  return (
    <footer className="transcript-footer" aria-label="Live transcription">
      <div className="transcript-footer__title">
        <AudioLines aria-hidden />
        <span>Live transcription</span>
        {liveActive ? <em>capturing voice</em> : null}
      </div>
      <ol className="transcript-footer__list" ref={listRef}>
        {visibleEntries.length === 0 ? (
          <li className="transcript-footer__empty">
            Listening for voice. The latest transcript appears here as soon as speech is decoded.
          </li>
        ) : (
          visibleEntries.map((entry) => (
            <li key={`${entry.chunk_id}:${entry.audio_path ?? entry.started_at}`} data-status={entry.status}>
              <span className="transcript-footer__status">
                {statusIcon[entry.status]}
                {statusLabel[entry.status]}
              </span>
              <span className="transcript-footer__text">
                {entry.status === "failed"
                  ? (entry.error ?? "transcription failed")
                  : entry.text || (
                      <em className="transcript-footer__placeholder">
                        {entry.status === "listening"
                          ? "listening to speech…"
                          : "processing…"}
                      </em>
                    )}
              </span>
            </li>
          ))
        )}
      </ol>
    </footer>
  );
}

function newestTranscriptTextEntries(entries: TranscriptEntry[]): TranscriptEntry[] {
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

export function scrollTranscriptListToBottom(
  list: Pick<HTMLOListElement, "scrollHeight" | "scrollTop"> | null,
) {
  if (!list) {
    return;
  }

  list.scrollTop = list.scrollHeight;
}
