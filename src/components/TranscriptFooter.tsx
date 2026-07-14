import { AlertCircle, AudioLines, CheckCircle2, Loader2 } from "lucide-react";
import { useEffect, useMemo, useRef, type ReactElement, type ReactNode } from "react";
import {
  appendRecentAge,
  fileUrlFromPath,
  formatLocalTimestamp,
} from "../lib/transcript-history";
import type { TranscriptEntry, TranscriptEntryStatus } from "../lib/transcript-log";
import { newestTranscriptTextEntries } from "../lib/live-transcripts";

const statusLabel: Record<TranscriptEntryStatus, string> = {
  listening: "Listening",
  partial: "Live",
  queued: "Refining",
  final: "Final",
  failed: "Review",
};

const statusIcon: Record<TranscriptEntryStatus, ReactElement> = {
  listening: <AudioLines aria-hidden />,
  partial: <Loader2 aria-hidden className="transcript-footer__spin" />,
  queued: <Loader2 aria-hidden className="transcript-footer__spin" />,
  final: <CheckCircle2 aria-hidden />,
  failed: <AlertCircle aria-hidden />,
};

export function TranscriptFooter({
  entries,
  liveActive,
  now = new Date(),
}: {
  entries: TranscriptEntry[];
  liveActive: boolean;
  now?: Date;
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
                {labelForEntry(entry)}
              </span>
              <span className="transcript-footer__text">
                {entry.status === "failed"
                  ? (entry.error ?? "transcription failed")
                  : textForEntry(entry, now) || (
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

function labelForEntry(entry: TranscriptEntry): ReactNode {
  if (entry.status !== "final") {
    return statusLabel[entry.status];
  }

  const timestamp = formatLocalTimestamp(entry.recorded_at || entry.started_at) || "Final";
  if (entry.audio_path) {
    return (
      <a href={fileUrlFromPath(entry.audio_path)} title={entry.audio_path}>
        {timestamp}
      </a>
    );
  }
  return timestamp;
}

function textForEntry(entry: TranscriptEntry, now: Date): string {
  if (entry.status !== "final" || !entry.text) {
    return entry.text;
  }

  return appendRecentAge(entry.text, entry.recorded_at || entry.started_at, now);
}

export function scrollTranscriptListToBottom(
  list: Pick<HTMLOListElement, "scrollHeight" | "scrollTop"> | null,
) {
  if (!list) {
    return;
  }

  list.scrollTop = list.scrollHeight;
}
