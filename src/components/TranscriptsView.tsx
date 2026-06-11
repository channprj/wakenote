import {
  Check,
  ChevronLeft,
  ChevronRight,
  Copy,
  Mic,
  MonitorSpeaker,
  Play,
  RotateCw,
  Video,
  X,
  Youtube,
} from "lucide-react";
import type {
  MouseEvent as ReactMouseEvent,
  PointerEvent as ReactPointerEvent,
} from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { audioPlaybackUrlFromPath } from "../lib/audio-playback";
import {
  fileUrlFromPath,
  formatLocalTimestamp,
  formatTranscriptsForCopy,
  transcriptSourceLabel,
} from "../lib/transcript-history";
import type { RecentTranscript, TranscriptDay } from "../lib/types";
import { Button } from "./ui/primitives";

type CopyToastKind = "all" | "selected";
type DragMode = "select" | "deselect";

interface DragState {
  mode: DragMode;
  visited: Set<string>;
}

interface TranscriptContextMenu {
  entry: RecentTranscript;
  x: number;
  y: number;
}

const KOREAN_DAY_LABELS = ["일", "월", "화", "수", "목", "금", "토"] as const;

export function TranscriptsView({
  days,
  entriesByDay,
  loadingDay = null,
  sourceLabels = {},
  onActiveDayChange,
  onRegenerate,
  onReload,
  initialPlayingTranscriptPath = null,
  today = new Date(),
}: {
  days: TranscriptDay[];
  entriesByDay: ReadonlyMap<string, RecentTranscript[]>;
  loadingDay?: string | null;
  sourceLabels?: Readonly<Record<string, string>>;
  onActiveDayChange?: (day: string) => void;
  onRegenerate?: (entry: RecentTranscript) => void | Promise<void>;
  onReload?: (day: string) => void;
  initialPlayingTranscriptPath?: string | null;
  today?: Date;
}) {
  const todayDay = formatLocalDay(today);

  const availableDays = useMemo(() => {
    const set = new Set<string>([todayDay]);
    for (const entry of days) {
      if (isYearMonthDayLabel(entry.day) && entry.day <= todayDay) {
        set.add(entry.day);
      }
    }
    return set;
  }, [days, todayDay]);

  const earliestDay = useMemo(() => {
    const sorted = [...availableDays].filter(isYearMonthDayLabel).sort();
    return sorted[0] ?? todayDay;
  }, [availableDays, todayDay]);

  const [activeDay, setActiveDay] = useState<string | null>(null);
  const [viewWeekStart, setViewWeekStart] = useState<string | null>(null);
  const [playingTranscriptPath, setPlayingTranscriptPath] = useState<string | null>(
    initialPlayingTranscriptPath,
  );
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(() => new Set());
  const [copyToast, setCopyToast] = useState<CopyToastKind | null>(null);
  const [contextMenu, setContextMenu] = useState<TranscriptContextMenu | null>(null);
  const dragStateRef = useRef<DragState | null>(null);

  const effectiveActiveDay = activeDay ?? todayDay;

  const activeEntries = useMemo(() => {
    const entries = entriesByDay.get(effectiveActiveDay) ?? [];
    return [...entries].sort(
      (left, right) =>
        timestampValue(left.recorded_at) - timestampValue(right.recorded_at) ||
        left.transcript_path.localeCompare(right.transcript_path),
    );
  }, [entriesByDay, effectiveActiveDay]);

  const playingTranscript = useMemo(() => {
    for (const list of entriesByDay.values()) {
      const match = list.find((entry) => entry.transcript_path === playingTranscriptPath);
      if (match) {
        return match;
      }
    }
    return undefined;
  }, [entriesByDay, playingTranscriptPath]);

  useEffect(() => {
    onActiveDayChange?.(effectiveActiveDay);
  }, [onActiveDayChange, effectiveActiveDay]);

  useEffect(() => {
    if (activeDay !== null && !availableDays.has(activeDay) && activeDay !== todayDay) {
      setActiveDay(null);
    }
  }, [availableDays, activeDay, todayDay]);

  useEffect(() => {
    setSelectedPaths(new Set());
    setCopyToast(null);
  }, [effectiveActiveDay]);

  useEffect(() => {
    const endDrag = () => {
      dragStateRef.current = null;
    };
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
    return () => {
      window.removeEventListener("pointerup", endDrag);
      window.removeEventListener("pointercancel", endDrag);
    };
  }, []);

  useEffect(() => {
    if (!contextMenu) {
      return;
    }
    const close = () => setContextMenu(null);
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        close();
      }
    };
    window.addEventListener("pointerdown", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [contextMenu]);

  const beginDragSelection = useCallback((path: string) => {
    setSelectedPaths((prev) => {
      const next = new Set(prev);
      const mode: DragMode = prev.has(path) ? "deselect" : "select";
      if (mode === "select") {
        next.add(path);
      } else {
        next.delete(path);
      }
      dragStateRef.current = { mode, visited: new Set([path]) };
      return next;
    });
  }, []);

  const continueDragSelection = useCallback((path: string) => {
    const drag = dragStateRef.current;
    if (!drag) return;
    if (drag.visited.has(path)) return;
    drag.visited.add(path);
    setSelectedPaths((prev) => {
      const next = new Set(prev);
      if (drag.mode === "select") {
        next.add(path);
      } else {
        next.delete(path);
      }
      return next;
    });
  }, []);

  const handleSelectDay = (day: string) => {
    setActiveDay(day);
    setViewWeekStart(null);
  };

  const writeToClipboard = useCallback(async (text: string, kind: CopyToastKind) => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopyToast(kind);
      setSelectedPaths(new Set());
      window.setTimeout(() => setCopyToast(null), 1500);
    } catch {
      // Clipboard API unavailable — silently ignore; UI feedback simply won't toggle.
    }
  }, []);

  const handleCopyAll = useCallback(() => {
    void writeToClipboard(formatTranscriptsForCopy(activeEntries, sourceLabels), "all");
  }, [writeToClipboard, activeEntries, sourceLabels]);

  const handleCopySelected = useCallback(() => {
    const selected = activeEntries.filter((entry) =>
      selectedPaths.has(entry.transcript_path),
    );
    void writeToClipboard(formatTranscriptsForCopy(selected, sourceLabels), "selected");
  }, [writeToClipboard, activeEntries, selectedPaths, sourceLabels]);

  const handleClearSelection = useCallback(() => {
    setSelectedPaths(new Set());
  }, []);

  const openContextMenu = useCallback((entry: RecentTranscript, x: number, y: number) => {
    const menuWidth = 168;
    const menuHeight = 44;
    const maxX = Math.max(8, window.innerWidth - menuWidth - 8);
    const maxY = Math.max(8, window.innerHeight - menuHeight - 8);
    setContextMenu({
      entry,
      x: Math.min(Math.max(8, x), maxX),
      y: Math.min(Math.max(8, y), maxY),
    });
  }, []);

  const handleRegenerateFromMenu = useCallback(() => {
    if (!contextMenu) {
      return;
    }
    const entry = contextMenu.entry;
    setContextMenu(null);
    void onRegenerate?.(entry);
  }, [contextMenu, onRegenerate]);

  const selectionCount = selectedPaths.size;
  const hasEntries = activeEntries.length > 0;
  const isLoadingActive = loadingDay === effectiveActiveDay;

  // The calendar count (size-based) can exceed the entries we managed to load
  // when some of the day's sidecars are iCloud-evicted (skipped on the
  // no-download navigation read). The gap is what an explicit reload would fetch.
  const availableCount = useMemo(
    () => days.find((entry) => entry.day === effectiveActiveDay)?.count ?? 0,
    [days, effectiveActiveDay],
  );
  const pendingCount = Math.max(0, availableCount - activeEntries.length);
  const hasPending = pendingCount > 0 && !isLoadingActive;

  const effectiveWeekStart = viewWeekStart ?? weekStartFor(effectiveActiveDay);
  const handlePrevWeek = () => setViewWeekStart(addDays(effectiveWeekStart, -7));
  const handleNextWeek = () => setViewWeekStart(addDays(effectiveWeekStart, 7));

  return (
    <div className="transcripts-panel">
      <TranscriptPagination
        activeDay={effectiveActiveDay}
        availableDays={availableDays}
        todayDay={todayDay}
        weekStart={effectiveWeekStart}
        earliestDay={earliestDay}
        onSelectDay={handleSelectDay}
        onPrevWeek={handlePrevWeek}
        onNextWeek={handleNextWeek}
      />
      <article className="transcript-day transcript-day--condensed">
        <header>
          <div>
            <span>{effectiveActiveDay}</span>
            <strong>
              {activeEntries.length} transcript
              {activeEntries.length === 1 ? "" : "s"}
            </strong>
          </div>
          <div className="transcript-day__actions">
            {hasEntries && selectionCount > 0 ? (
              <>
                <span aria-live="polite" className="transcript-day__selection-count">
                  {selectionCount} 선택됨
                </span>
                <Button onClick={handleClearSelection} size="sm" type="button" variant="ghost">
                  선택 해제
                </Button>
                <Button
                  aria-label="선택한 트랜스크립트 복사"
                  onClick={handleCopySelected}
                  size="sm"
                  type="button"
                  variant="secondary"
                >
                  {copyToast === "selected" ? (
                    <>
                      <Check /> 복사됨
                    </>
                  ) : (
                    <>
                      <Copy /> 선택 복사
                    </>
                  )}
                </Button>
              </>
            ) : null}
            {hasEntries ? (
              <Button
                aria-label="해당 일자의 모든 트랜스크립트 복사"
                onClick={handleCopyAll}
                size="sm"
                type="button"
                variant="secondary"
              >
                {copyToast === "all" ? (
                  <>
                    <Check /> 복사됨
                  </>
                ) : (
                  <>
                    <Copy /> 전체 복사
                  </>
                )}
              </Button>
            ) : null}
            {hasPending && hasEntries ? (
              <span className="transcript-day__icloud-hint">
                iCloud에 {pendingCount}개 더 있음
              </span>
            ) : null}
            <Button
              aria-label="해당 일자 다시 불러오기"
              disabled={isLoadingActive}
              onClick={() => onReload?.(effectiveActiveDay)}
              size="sm"
              type="button"
              variant="ghost"
            >
              <RotateCw /> 다시 불러오기
            </Button>
          </div>
        </header>
        {hasEntries ? (
          <div className="transcript-entry-list transcript-entry-list--condensed">
            {activeEntries.map((entry) => (
              <TranscriptEntryRow
                entry={entry}
                isPlaying={entry.transcript_path === playingTranscriptPath}
                isSelected={selectedPaths.has(entry.transcript_path)}
                key={entry.transcript_path}
                sourceLabels={sourceLabels}
                onOpenContextMenu={openContextMenu}
                onPlay={() => setPlayingTranscriptPath(entry.transcript_path)}
                onPointerDownSelect={beginDragSelection}
                onPointerEnterSelect={continueDragSelection}
                onRegenerate={onRegenerate}
              />
            ))}
          </div>
        ) : (
          <div className="transcripts-empty">
            {isLoadingActive
              ? "Loading…"
              : hasPending
                ? `iCloud에 ${pendingCount}개 있습니다 — 다시 불러오기를 누르세요`
                : "No transcripts for this day"}
          </div>
        )}
      </article>
      {playingTranscript?.audio_path ? (
        <TranscriptPlayerSheet
          entry={playingTranscript}
          onClose={() => setPlayingTranscriptPath(null)}
        />
      ) : null}
      {contextMenu ? (
        <div
          className="transcript-context-menu"
          role="menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(event) => event.stopPropagation()}
        >
          <button onClick={handleRegenerateFromMenu} role="menuitem" type="button">
            <RotateCw aria-hidden="true" />
            Regenerate
          </button>
        </div>
      ) : null}
    </div>
  );
}

function TranscriptEntryRow({
  entry,
  isPlaying,
  isSelected,
  sourceLabels,
  onOpenContextMenu,
  onPlay,
  onPointerDownSelect,
  onPointerEnterSelect,
  onRegenerate,
}: {
  entry: RecentTranscript;
  isPlaying: boolean;
  isSelected: boolean;
  sourceLabels: Readonly<Record<string, string>>;
  onOpenContextMenu: (entry: RecentTranscript, x: number, y: number) => void;
  onPlay: () => void;
  onPointerDownSelect: (path: string) => void;
  onPointerEnterSelect: (path: string) => void;
  onRegenerate?: (entry: RecentTranscript) => void | Promise<void>;
}) {
  const timestamp = formatLocalTimestamp(entry.recorded_at);
  const regenerateAvailable = Boolean(entry.audio_path && onRegenerate);

  const handlePointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest("a, button")) return;
    event.preventDefault();
    onPointerDownSelect(entry.transcript_path);
  };

  const handlePointerEnter = () => {
    onPointerEnterSelect(entry.transcript_path);
  };

  const handleContextMenu = (event: ReactMouseEvent<HTMLDivElement>) => {
    if (!regenerateAvailable) {
      return;
    }
    const target = event.target as HTMLElement | null;
    if (target?.closest("a, button")) {
      return;
    }
    event.preventDefault();
    onOpenContextMenu(entry, event.clientX, event.clientY);
  };

  return (
    <div
      aria-selected={isSelected}
      className="transcript-entry transcript-entry--condensed"
      data-audio-path={regenerateAvailable ? entry.audio_path ?? undefined : undefined}
      data-regenerate-available={regenerateAvailable ? "true" : undefined}
      data-selected={isSelected ? "true" : undefined}
      onContextMenu={handleContextMenu}
      onPointerDown={handlePointerDown}
      onPointerEnter={handlePointerEnter}
    >
      <a
        className="transcript-entry__timestamp"
        href={fileUrlFromPath(entry.transcript_path)}
        title={entry.transcript_path}
      >
        <span>{timestamp || entry.transcript_path}</span>
      </a>
      <TranscriptSourceBadge entry={entry} sourceLabels={sourceLabels} />
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

function TranscriptSourceBadge({
  entry,
  sourceLabels,
}: {
  entry: RecentTranscript;
  sourceLabels: Readonly<Record<string, string>>;
}) {
  const presentation = transcriptSourcePresentation(entry, sourceLabels);
  const Icon = presentation.icon;
  return (
    <span
      className={`transcript-source-badge transcript-source-badge--${presentation.tone}`}
      title={presentation.title}
    >
      <Icon aria-hidden="true" />
      {presentation.label}
    </span>
  );
}

function transcriptSourcePresentation(
  entry: RecentTranscript,
  sourceLabels: Readonly<Record<string, string>>,
): {
  label: string;
  title: string;
  tone: "microphone" | "youtube" | "meet" | "system";
  icon: typeof Mic;
} {
  if ((entry.source ?? "microphone") !== "system") {
    return {
      label: "Mic",
      title: "Microphone transcript",
      tone: "microphone",
      icon: Mic,
    };
  }

  const label = transcriptSourceLabel(entry, sourceLabels);
  if (entry.source_label === "youtube") {
    return {
      label,
      title: "YouTube system-audio transcript",
      tone: "youtube",
      icon: Youtube,
    };
  }
  if (entry.source_label === "meet") {
    return {
      label,
      title: "Google Meet system-audio transcript",
      tone: "meet",
      icon: Video,
    };
  }
  return {
    label,
    title: `${label} system-audio transcript`,
    tone: "system",
    icon: MonitorSpeaker,
  };
}

export function formatLocalDay(date: Date): string {
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

function timestampValue(value: string): number {
  const time = new Date(value).getTime();
  return Number.isNaN(time) ? Number.MAX_SAFE_INTEGER : time;
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
  availableDays,
  todayDay,
  weekStart,
  earliestDay,
  onSelectDay,
  onPrevWeek,
  onNextWeek,
}: {
  activeDay: string;
  availableDays: ReadonlySet<string>;
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
          const hasEntries = availableDays.has(day);
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
