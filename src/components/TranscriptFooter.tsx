import { AlertCircle, AudioLines, CheckCircle2, Loader2 } from "lucide-react";
import type { ReactElement } from "react";
import type { TranscriptEntry, TranscriptEntryStatus } from "../lib/transcript-log";

const statusLabel: Record<TranscriptEntryStatus, string> = {
  listening: "Listening",
  partial: "Partial",
  queued: "Queued",
  final: "Final",
  failed: "Failed",
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
}: {
  entries: TranscriptEntry[];
  liveActive: boolean;
}) {
  if (entries.length === 0 && !liveActive) {
    return null;
  }

  return (
    <footer className="transcript-footer" aria-label="Live transcription">
      <div className="transcript-footer__title">
        <AudioLines aria-hidden />
        <span>Live transcription</span>
        {liveActive ? <em>capturing</em> : null}
      </div>
      <ol className="transcript-footer__list">
        {entries.length === 0 ? (
          <li className="transcript-footer__empty">
            Waiting for the first speech segment&hellip;
          </li>
        ) : (
          entries.map((entry) => (
            <li key={entry.chunk_id} data-status={entry.status}>
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
