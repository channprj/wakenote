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

const KOREAN_DAY_LABELS = ["일", "월", "화", "수", "목", "금", "토"] as const;

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
  const [activeDay, setActiveDay] = useState<string | null>(null);
  const [viewWeekStart, setViewWeekStart] = useState<string | null>(null);
  const [playingTranscriptPath, setPlayingTranscriptPath] = useState<string | null>(
    initialPlayingTranscriptPath,
  );

  const effectiveActiveDay = activeDay ?? todayDay;
  const pageByDay = useMemo(
    () => new Map(pages.map((page) => [page.day, page] as const)),
    [pages],
  );
  const activeGroup: TranscriptDatePage =
    pageByDay.get(effectiveActiveDay) ?? { day: effectiveActiveDay, entries: [] };
  const playingTranscript = transcripts.find(
    (entry) => entry.transcript_path === playingTranscriptPath,
  );

  useEffect(() => {
    if (activeDay !== null && !pageByDay.has(activeDay) && activeDay !== todayDay) {
      setActiveDay(null);
    }
  }, [pageByDay, activeDay, todayDay]);

  const handleSelectDay = (day: string) => {
    setActiveDay(day);
    setViewWeekStart(null);
  };

  const effectiveWeekStart = viewWeekStart ?? weekStartFor(effectiveActiveDay);

  const handlePrevWeek = () => {
    setViewWeekStart(addDays(effectiveWeekStart, -7));
  };
  const handleNextWeek = () => {
    setViewWeekStart(addDays(effectiveWeekStart, 7));
  };

  return (
    <div className="transcripts-panel">
      <TranscriptPagination
        activeDay={effectiveActiveDay}
        pageByDay={pageByDay}
        todayDay={todayDay}
        weekStart={effectiveWeekStart}
        earliestDay={pages[0]?.day ?? todayDay}
        onSelectDay={handleSelectDay}
        onPrevWeek={handlePrevWeek}
        onNextWeek={handleNextWeek}
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
  const days = new Set<string>([todayDay]);
  for (const day of groupsByDay.keys()) {
    if (isYearMonthDayLabel(day) && day <= todayDay) {
      days.add(day);
    }
  }

  return [...days]
    .sort()
    .map((day) => ({ day, entries: groupsByDay.get(day) ?? [] }));
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

function isYearMonthDayLabel(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value);
}

function pad2(value: number): string {
  return String(value).padStart(2, "0");
}

function parseDay(day: string): Date {
  const [year, month, date] = day.split("-").map(Number);
  return new Date(year, month - 1, date);
}

export function addDays(day: string, count: number): string {
  const date = parseDay(day);
  date.setDate(date.getDate() + count);
  return formatLocalDay(date);
}

export function weekStartFor(day: string): string {
  const date = parseDay(day);
  date.setDate(date.getDate() - date.getDay());
  return formatLocalDay(date);
}

export function previousWeekDisabledReason(
  weekStart: string,
  earliestDay: string,
): string | null {
  if (weekStart <= weekStartFor(earliestDay)) {
    return "Already on the earliest week";
  }
  return null;
}

export function nextWeekDisabledReason(weekStart: string, todayDay: string): string | null {
  if (weekStart >= weekStartFor(todayDay)) {
    return "Already on this week";
  }
  return null;
}

function TranscriptPagination({
  activeDay,
  pageByDay,
  todayDay,
  weekStart,
  earliestDay,
  onSelectDay,
  onPrevWeek,
  onNextWeek,
}: {
  activeDay: string;
  pageByDay: Map<string, TranscriptDatePage>;
  todayDay: string;
  weekStart: string;
  earliestDay: string;
  onSelectDay: (day: string) => void;
  onPrevWeek: () => void;
  onNextWeek: () => void;
}) {
  const previousReason = previousWeekDisabledReason(weekStart, earliestDay);
  const nextReason = nextWeekDisabledReason(weekStart, todayDay);
  const weekDays = Array.from({ length: 7 }, (_, i) => addDays(weekStart, i));

  return (
    <nav
      className="transcript-pagination transcript-pagination--calendar"
      aria-label="Transcript date pages"
    >
      <Button
        aria-label="Previous week"
        disabled={previousReason !== null}
        onClick={onPrevWeek}
        size="icon"
        title={previousReason ?? undefined}
        type="button"
        variant="secondary"
      >
        <ChevronLeft />
      </Button>
      <div className="transcript-pagination__week">
        {weekDays.map((day, dayOfWeek) => {
          const hasEntries = pageByDay.has(day);
          const isFuture = day > todayDay;
          const isToday = day === todayDay;
          const selectable = !isFuture && (hasEntries || isToday);
          const isActive = activeDay === day;
          const dayNumber = day.split("-")[2];
          const monthNumber = day.split("-")[1];

          return (
            <button
              key={day}
              aria-current={isActive ? "page" : undefined}
              aria-label={`Go to ${day} transcripts`}
              className="transcript-pagination__day"
              data-day-of-week={dayOfWeek}
              data-has-entries={hasEntries ? "true" : undefined}
              data-today={isToday ? "true" : undefined}
              disabled={!selectable}
              onClick={() => onSelectDay(day)}
              type="button"
            >
              <span className="transcript-pagination__day-label">
                {KOREAN_DAY_LABELS[dayOfWeek]}
              </span>
              <span className="transcript-pagination__day-number">
                {monthNumber}/{dayNumber}
              </span>
            </button>
          );
        })}
      </div>
      <Button
        aria-label="Next week"
        disabled={nextReason !== null}
        onClick={onNextWeek}
        size="icon"
        title={nextReason ?? undefined}
        type="button"
        variant="secondary"
      >
        <ChevronRight />
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
