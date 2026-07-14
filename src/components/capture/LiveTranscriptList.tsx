import { AudioLinesIcon } from "lucide-react";
import type { ReactNode } from "react";
import {
  Empty,
  EmptyDescription,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";
import { newestTranscriptTextEntries } from "@/lib/live-transcripts";
import {
  appendRecentAge,
  fileUrlFromPath,
  formatLocalTimestamp,
} from "@/lib/transcript-history";
import type {
  TranscriptEntry,
  TranscriptEntryStatus,
} from "@/lib/transcript-log";

const STATUS_LABEL: Record<TranscriptEntryStatus, string> = {
  listening: "Listening",
  partial: "Live",
  queued: "Refining",
  final: "Final",
  failed: "Review",
};

export function LiveTranscriptList({
  entries,
  liveActive,
  now = new Date(),
}: {
  entries: readonly TranscriptEntry[];
  liveActive: boolean;
  now?: Date;
}) {
  const visibleEntries = newestTranscriptTextEntries(entries);

  if (visibleEntries.length === 0) {
    return (
      <Empty className="live-transcript-empty">
        <EmptyHeader>
          <EmptyMedia variant="icon">
            <AudioLinesIcon />
          </EmptyMedia>
          <EmptyTitle>
            {liveActive ? "Listening for speech" : "No speech captured yet"}
          </EmptyTitle>
          <EmptyDescription>
            {liveActive
              ? "The latest decoded phrase will appear here."
              : "Start input to build a live transcript."}
          </EmptyDescription>
        </EmptyHeader>
      </Empty>
    );
  }

  return (
    <ol data-slot="live-transcript-list" aria-label="Live transcription">
      {visibleEntries.map((entry) => (
        <li
          key={`${entry.chunk_id}:${entry.audio_path ?? entry.started_at}`}
          data-status={entry.status}
        >
          <span data-slot="transcript-meta">{labelForEntry(entry)}</span>
          <span data-slot="transcript-text">{textForEntry(entry, now)}</span>
        </li>
      ))}
    </ol>
  );
}

function labelForEntry(entry: TranscriptEntry): ReactNode {
  if (entry.status !== "final") {
    return STATUS_LABEL[entry.status];
  }

  const timestamp = formatLocalTimestamp(entry.recorded_at || entry.started_at) || "Final";
  if (!entry.audio_path) {
    return timestamp;
  }
  return (
    <a href={fileUrlFromPath(entry.audio_path)} title={entry.audio_path}>
      {timestamp}
    </a>
  );
}

function textForEntry(entry: TranscriptEntry, now: Date): string {
  if (entry.status !== "final") {
    return entry.text;
  }
  return appendRecentAge(entry.text, entry.recorded_at || entry.started_at, now);
}
