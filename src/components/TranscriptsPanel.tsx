import { ChevronLeft, ChevronRight, FileAudio, FileText, Play, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { fileUrlFromPath, formatLocalTimestamp, groupTranscriptsByDay } from "../lib/transcript-history";
import type { RecentTranscript } from "../lib/types";
import { Badge, Button } from "./ui/primitives";

export function TranscriptsPanel({
  transcripts,
  initialPlayingTranscriptPath = null,
}: {
  transcripts: RecentTranscript[];
  initialPlayingTranscriptPath?: string | null;
}) {
  const groups = useMemo(() => groupTranscriptsByDay(transcripts), [transcripts]);
  const [activePage, setActivePage] = useState(0);
  const [playingTranscriptPath, setPlayingTranscriptPath] = useState<string | null>(
    initialPlayingTranscriptPath,
  );
  const activeGroup = groups[activePage];
  const playingTranscript = transcripts.find(
    (entry) => entry.transcript_path === playingTranscriptPath,
  );

  useEffect(() => {
    setActivePage((page) => Math.min(page, Math.max(0, groups.length - 1)));
  }, [groups.length]);

  if (groups.length === 0) {
    return (
      <div className="transcripts-empty">
        No saved transcripts
      </div>
    );
  }

  return (
    <div className="transcripts-panel">
      {activeGroup ? (
        <>
          <TranscriptPagination
            activePage={activePage}
            groups={groups}
            onPageChange={setActivePage}
          />
          <article className="transcript-day">
            <header>
              <div>
                <span>{activeGroup.day}</span>
                <strong>{activeGroup.entries.length} transcript{activeGroup.entries.length === 1 ? "" : "s"}</strong>
              </div>
              <Badge tone="primary">Daily transcript</Badge>
            </header>
            <div className="transcript-entry-list">
              {activeGroup.entries.map((entry) => (
                <TranscriptEntryRow
                  entry={entry}
                  isPlaying={entry.transcript_path === playingTranscriptPath}
                  key={entry.transcript_path}
                  onPlay={() => setPlayingTranscriptPath(entry.transcript_path)}
                />
              ))}
            </div>
          </article>
          <TranscriptPagination
            activePage={activePage}
            groups={groups}
            onPageChange={setActivePage}
          />
        </>
      ) : null}
      {playingTranscript?.audio_path ? (
        <TranscriptPlayerSheet
          entry={playingTranscript}
          onClose={() => setPlayingTranscriptPath(null)}
        />
      ) : null}
    </div>
  );
}

function TranscriptEntryRow({
  entry,
  isPlaying,
  onPlay,
}: {
  entry: RecentTranscript;
  isPlaying: boolean;
  onPlay: () => void;
}) {
  const timestamp = formatLocalTimestamp(entry.recorded_at);

  return (
    <div className="transcript-entry">
      <div className="transcript-entry__body">
        <div className="transcript-entry__meta">
          <FileText aria-hidden />
          <span title={entry.transcript_path}>{timestamp || entry.transcript_path}</span>
        </div>
        <p>{entry.text}</p>
      </div>
      <div className="transcript-entry__actions">
        {entry.audio_path ? (
          <a href={fileUrlFromPath(entry.audio_path)} title={entry.audio_path}>
            <FileAudio aria-hidden />
            Recording
          </a>
        ) : null}
        <Button
          aria-label={timestamp ? `Play recording from ${timestamp}` : "Play recording"}
          aria-pressed={isPlaying}
          className="transcript-entry__play"
          disabled={!entry.audio_path}
          onClick={onPlay}
          size="icon"
          title={entry.audio_path ? "Play recording" : "No recording file"}
          type="button"
          variant={isPlaying ? "primary" : "secondary"}
        >
          <Play />
        </Button>
      </div>
    </div>
  );
}

function TranscriptPagination({
  activePage,
  groups,
  onPageChange,
}: {
  activePage: number;
  groups: ReturnType<typeof groupTranscriptsByDay>;
  onPageChange: (page: number) => void;
}) {
  if (groups.length <= 1) {
    return null;
  }

  return (
    <nav className="transcript-pagination" aria-label="Transcript date pages">
      <Button
        disabled={activePage === 0}
        onClick={() => onPageChange(Math.max(0, activePage - 1))}
        type="button"
        variant="secondary"
      >
        <ChevronLeft data-icon="inline-start" />
        Previous day
      </Button>
      <div className="transcript-pagination__pages">
        {groups.map((group, index) => (
          <button
            aria-current={activePage === index ? "page" : undefined}
            aria-label={`Go to page ${index + 1}, ${group.day}`}
            key={group.day}
            onClick={() => onPageChange(index)}
            type="button"
          >
            {index + 1}
          </button>
        ))}
      </div>
      <span>Page {activePage + 1} of {groups.length}</span>
      <Button
        disabled={activePage === groups.length - 1}
        onClick={() => onPageChange(Math.min(groups.length - 1, activePage + 1))}
        type="button"
        variant="secondary"
      >
        Next day
        <ChevronRight data-icon="inline-end" />
      </Button>
    </nav>
  );
}

function TranscriptPlayerSheet({
  entry,
  onClose,
}: {
  entry: RecentTranscript;
  onClose: () => void;
}) {
  if (!entry.audio_path) {
    return null;
  }

  const timestamp = formatLocalTimestamp(entry.recorded_at);

  return (
    <aside className="transcript-player-sheet" aria-label="Transcript player">
      <div className="transcript-player-sheet__header">
        <div>
          <span>Now playing</span>
          <strong>{timestamp || entry.audio_path}</strong>
        </div>
        <Button
          aria-label="Close player"
          onClick={onClose}
          size="icon"
          type="button"
          variant="ghost"
        >
          <X />
        </Button>
      </div>
      <audio autoPlay controls src={fileUrlFromPath(entry.audio_path)} />
      <p>{entry.text}</p>
    </aside>
  );
}
