import { ChevronLeft, ChevronRight, Play, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { audioPlaybackUrlFromPath } from "../lib/audio-playback";
import {
  fileUrlFromPath,
  formatLocalTimestamp,
  groupTranscriptsByDay,
} from "../lib/transcript-history";
import type { RecentTranscript } from "../lib/types";
import { Button } from "./ui/primitives";

interface TranscriptDatePage {
  day: string;
  entries: RecentTranscript[];
}

export function TranscriptsPanel({
  transcripts,
  initialPlayingTranscriptPath = null,
  today = new Date(),
}: {
  transcripts: RecentTranscript[];
  initialPlayingTranscriptPath?: string | null;
  today?: Date;
}) {
  const todayDay = formatLocalDay(today);
  const pages = useMemo(
    () => buildTranscriptDatePages(transcripts, todayDay),
    [transcripts, todayDay],
  );
  const [activePage, setActivePage] = useState(0);
  const [playingTranscriptPath, setPlayingTranscriptPath] = useState<string | null>(
    initialPlayingTranscriptPath,
  );
  const activeGroup = pages[activePage] ?? pages[0];
  const playingTranscript = transcripts.find(
    (entry) => entry.transcript_path === playingTranscriptPath,
  );

  useEffect(() => {
    setActivePage((page) => Math.min(page, Math.max(0, pages.length - 1)));
  }, [pages.length]);

  return (
    <div className="transcripts-panel">
      {activeGroup ? (
        <>
          <TranscriptPagination
            activePage={activePage}
            activeDay={activeGroup.day}
            pages={pages}
            onPageChange={setActivePage}
          />
          <article className="transcript-day transcript-day--condensed">
            <header>
              <div>
                <span>{activeGroup.day}</span>
                <strong>
                  {activeGroup.entries.length} transcript
                  {activeGroup.entries.length === 1 ? "" : "s"}
                </strong>
              </div>
            </header>
            {activeGroup.entries.length > 0 ? (
              <div className="transcript-entry-list transcript-entry-list--condensed">
                {activeGroup.entries.map((entry) => (
                  <TranscriptEntryRow
                    entry={entry}
                    isPlaying={entry.transcript_path === playingTranscriptPath}
                    key={entry.transcript_path}
                    onPlay={() => setPlayingTranscriptPath(entry.transcript_path)}
                  />
                ))}
              </div>
            ) : (
              <div className="transcripts-empty">No transcripts for this day</div>
            )}
          </article>
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
    <div className="transcript-entry transcript-entry--condensed">
      <a
        className="transcript-entry__timestamp"
        href={fileUrlFromPath(entry.transcript_path)}
        title={entry.transcript_path}
      >
        <span>{timestamp || entry.transcript_path}</span>
      </a>
      <p className="transcript-entry__text">{entry.text}</p>
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
  );
}

function buildTranscriptDatePages(
  transcripts: RecentTranscript[],
  todayDay: string,
): TranscriptDatePage[] {
  const groupsByDay = new Map(
    groupTranscriptsByDay(transcripts).map((group) => [group.day, group.entries] as const),
  );
  const datedTranscriptDays = [...groupsByDay.keys()]
    .filter(isYearMonthDayLabel)
    .filter((day) => day <= todayDay)
    .sort();
  const earliestDay = datedTranscriptDays[0] ?? todayDay;
  const pages: TranscriptDatePage[] = [];

  for (let day = todayDay; day >= earliestDay; day = previousLocalDay(day)) {
    pages.push({ day, entries: groupsByDay.get(day) ?? [] });
    if (day === earliestDay) {
      break;
    }
  }

  return pages.length > 0 ? pages : [{ day: todayDay, entries: [] }];
}

function formatLocalDay(date: Date): string {
  if (Number.isNaN(date.getTime())) {
    return formatLocalDay(new Date());
  }

  return [
    date.getFullYear(),
    pad2(date.getMonth() + 1),
    pad2(date.getDate()),
  ].join("-");
}

function previousLocalDay(day: string): string {
  const date = localDateFromDay(day);
  date.setDate(date.getDate() - 1);
  return formatLocalDay(date);
}

function localDateFromDay(day: string): Date {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(year, month - 1, date);
}

function isYearMonthDayLabel(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value);
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

export function previousDayDisabledReason(activePage: number, pagesLength: number): string | null {
  if (activePage >= pagesLength - 1) {
    return "Already on the earliest transcript date";
  }
  return null;
}

export function nextDayDisabledReason(activePage: number): string | null {
  if (activePage <= 0) {
    return "Already on today";
  }
  return null;
}

function TranscriptPagination({
  activePage,
  activeDay,
  pages,
  onPageChange,
}: {
  activePage: number;
  activeDay: string;
  pages: TranscriptDatePage[];
  onPageChange: (page: number) => void;
}) {
  if (pages.length <= 1) {
    return null;
  }

  const previousReason = previousDayDisabledReason(activePage, pages.length);
  const nextReason = nextDayDisabledReason(activePage);

  return (
    <nav
      className="transcript-pagination transcript-pagination--calendar"
      aria-label="Transcript date pages"
    >
      <Button
        aria-label="Previous date"
        disabled={activePage === pages.length - 1}
        onClick={() => onPageChange(Math.min(pages.length - 1, activePage + 1))}
        title={previousReason ?? undefined}
        type="button"
        variant="secondary"
      >
        <ChevronLeft data-icon="inline-start" />
        Previous date
      </Button>
      <div className="transcript-pagination__pages">
        {pages.map((group, index) => (
          <button
            aria-current={activePage === index ? "page" : undefined}
            aria-label={`Go to ${group.day} transcripts`}
            key={group.day}
            onClick={() => onPageChange(index)}
            type="button"
          >
            {group.day}
          </button>
        ))}
      </div>
      <span>{activeDay} / {pages.length} days</span>
      <Button
        aria-label="Next date"
        disabled={activePage === 0}
        onClick={() => onPageChange(Math.max(0, activePage - 1))}
        title={nextReason ?? undefined}
        type="button"
        variant="secondary"
      >
        Next date
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
  const audioPath = entry.audio_path;

  if (!audioPath) {
    return null;
  }

  const timestamp = formatLocalTimestamp(entry.recorded_at);
  const fallbackAudioSource = fileUrlFromPath(audioPath);
  const [audioSource, setAudioSource] = useState(fallbackAudioSource);
  const [audioError, setAudioError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const fallback = fileUrlFromPath(audioPath);
    setAudioSource(fallback);
    setAudioError(null);

    audioPlaybackUrlFromPath(audioPath)
      .then((url) => {
        if (!cancelled) {
          setAudioSource(url);
        }
      })
      .catch((error: unknown) => {
        if (!cancelled) {
          setAudioError(error instanceof Error ? error.message : "Could not prepare recording");
        }
      });

    return () => {
      cancelled = true;
    };
  }, [audioPath]);

  return (
    <aside className="transcript-player-sheet" aria-label="Transcript player">
      <div className="transcript-player-sheet__header">
        <div>
          <span>Now playing</span>
          <a href={fileUrlFromPath(audioPath)} title={audioPath}>
            <strong>{timestamp || audioPath}</strong>
          </a>
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
      <audio autoPlay controls key={audioSource} preload="metadata" src={audioSource} />
      {audioError ? (
        <span className="transcript-player-sheet__error">{audioError}</span>
      ) : null}
      <p>{entry.text}</p>
    </aside>
  );
}
