import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { RecentTranscript, TranscriptDay } from "../lib/types";
import {
  TranscriptsView,
  addDays,
  nextWeekDisabledReason,
  previousWeekDisabledReason,
  weekStartFor,
} from "./TranscriptsView";

function transcript(overrides: Partial<RecentTranscript>): RecentTranscript {
  return {
    transcript_path: "/tmp/WakeNote/20260510/010203.txt",
    audio_path: "/tmp/WakeNote/20260510/010203.m4a",
    recorded_at: "2026-05-10T01:02:03+09:00",
    text: "Daily transcript text",
    ...overrides,
  };
}

function view(props: {
  today?: Date;
  days?: TranscriptDay[];
  entriesByDay?: Map<string, RecentTranscript[]>;
  loadingDay?: string | null;
  initialPlayingTranscriptPath?: string | null;
}) {
  return renderToStaticMarkup(
    <TranscriptsView
      days={props.days ?? []}
      entriesByDay={props.entriesByDay ?? new Map()}
      loadingDay={props.loadingDay ?? null}
      initialPlayingTranscriptPath={props.initialPlayingTranscriptPath}
      today={props.today}
    />,
  );
}

describe("TranscriptsView", () => {
  it("shows today's date page by default with weekly calendar pagination", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [
        { day: "2026-05-13", count: 1 },
        { day: "2026-05-14", count: 1 },
      ],
      entriesByDay: new Map([
        ["2026-05-13", [transcript({ recorded_at: "2026-05-13T01:02:03+09:00", text: "yesterday transcript" })]],
        ["2026-05-14", [transcript({ recorded_at: "2026-05-14T01:02:03+09:00", text: "today transcript" })]],
      ]),
    });

    expect(markup).toContain("2026-05-14");
    expect(markup).toContain("today transcript");
    expect(markup).not.toContain("yesterday transcript");
    expect(markup).toContain('aria-label="Go to 2026-05-10 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-13 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-14 transcripts"');
    expect(markup).toContain('aria-label="Go to 2026-05-16 transcripts"');
    expect(markup).toContain('aria-label="Previous week"');
    expect(markup).toContain('aria-label="Next week"');
    expect(markup).toContain('aria-current="page"');
    expect(markup).toContain("transcript-pagination--calendar");
  });

  it("renders week-day cells in 일-월-화-수-목-금-토 order with weekend tone hooks", () => {
    const markup = view({ today: new Date("2026-05-14T12:00:00+09:00") });
    const labels = ["일", "월", "화", "수", "목", "금", "토"];
    let cursor = -1;
    for (const label of labels) {
      const next = markup.indexOf(`>${label}<`, cursor + 1);
      expect(next, `expected ${label} after previous label`).toBeGreaterThan(cursor);
      cursor = next;
    }
    expect(markup).toContain('data-day-of-week="0"');
    expect(markup).toContain('data-day-of-week="6"');
  });

  it("disables future days within the current week and the Next-week arrow", () => {
    const markup = view({ today: new Date("2026-05-14T12:00:00+09:00") });
    expect(markup).toMatch(/<button[^>]*aria-label="Go to 2026-05-15 transcripts"[^>]*disabled=""/);
    expect(markup).toMatch(/<button[^>]*aria-label="Go to 2026-05-16 transcripts"[^>]*disabled=""/);
    expect(markup).toMatch(/<button[^>]*aria-label="Next week"[^>]*disabled=""[^>]*title="Already on this week"/);
  });

  it("disables the Previous-week arrow on the earliest week with transcripts", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-13", count: 1 }],
    });
    expect(markup).toMatch(/<button[^>]*aria-label="Previous week"[^>]*disabled=""[^>]*title="Already on the earliest week"/);
  });

  it("starts on today's empty date instead of the newest saved day", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "older saved transcript" })]]]),
    });
    expect(markup).toContain("2026-05-14");
    expect(markup).toContain("No transcripts for this day");
    expect(markup).not.toContain("older saved transcript");
    expect(markup).toContain('aria-label="Go to 2026-05-10 transcripts"');
  });

  it("renders transcripts in chronological order within the selected (today) date", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 2 }],
      entriesByDay: new Map([
        ["2026-05-10", [
          transcript({ transcript_path: "/tmp/WakeNote/20260510/180000.txt", recorded_at: "2026-05-10T18:00:00+09:00", text: "evening transcript" }),
          transcript({ transcript_path: "/tmp/WakeNote/20260510/090000.txt", recorded_at: "2026-05-10T09:00:00+09:00", text: "morning transcript" }),
        ]],
      ]),
    });
    expect(markup.indexOf("morning transcript")).toBeLessThan(markup.indexOf("evening transcript"));
  });

  it("renders a play button and inline file:// timestamp link for each transcript", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "playable transcript" })]]]),
    });
    expect(markup).toContain("playable transcript");
    expect(markup).toContain('aria-label="Play recording from 2026-05-10 01:02:03"');
    expect(markup).toContain("transcript-entry__play");
    expect(markup).toContain('<a class="transcript-entry__timestamp" href="file:///tmp/WakeNote/20260510/010203.txt" title="/tmp/WakeNote/20260510/010203.txt"><span>2026-05-10 01:02:03</span></a>');
  });

  it("always renders a reload button and shows copy actions only when the day has entries", () => {
    const withEntries = view({
      today: new Date("2026-05-19T18:00:00+09:00"),
      days: [{ day: "2026-05-19", count: 1 }],
      entriesByDay: new Map([["2026-05-19", [transcript({ recorded_at: "2026-05-19T15:53:23+09:00", text: "슬립~" })]]]),
    });
    expect(withEntries).toContain('aria-label="해당 일자 다시 불러오기"');
    expect(withEntries).toContain('aria-label="해당 일자의 모든 트랜스크립트 복사"');
    expect(withEntries).toContain("전체 복사");

    const empty = view({ today: new Date("2026-05-19T18:00:00+09:00") });
    expect(empty).toContain('aria-label="해당 일자 다시 불러오기"');
    expect(empty).not.toContain("전체 복사");
  });

  it("disables the reload button while the active day is loading", () => {
    const markup = view({
      today: new Date("2026-05-19T18:00:00+09:00"),
      loadingDay: "2026-05-19",
    });
    expect(markup).toMatch(/<button[^>]*aria-label="해당 일자 다시 불러오기"[^>]*disabled=""/);
    expect(markup).toContain("Loading…");
  });

  it("shows a bottom sheet player while a transcript is selected for playback", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "currently playing transcript" })]]]),
      initialPlayingTranscriptPath: "/tmp/WakeNote/20260510/010203.txt",
    });
    expect(markup).toContain("transcript-player-sheet");
    expect(markup).toContain("currently playing transcript");
    expect(markup).toContain('src="file:///tmp/WakeNote/20260510/010203.m4a"');
    expect(markup).toContain("Now playing");
  });

  it("hints that more entries are in iCloud when the day count exceeds loaded entries", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-14", count: 3 }],
      entriesByDay: new Map([
        ["2026-05-14", [transcript({ recorded_at: "2026-05-14T01:02:03+09:00", text: "local one" })]],
      ]),
    });
    // 3 in the (size-based) count, 1 loaded → 2 still in iCloud.
    expect(markup).toContain("iCloud에 2개 더 있음");
    expect(markup).toContain("local one");
    expect(markup).toContain('aria-label="해당 일자 다시 불러오기"');
  });

  it("prompts a reload in the empty state when a day is entirely iCloud-evicted", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-14", count: 5 }],
      entriesByDay: new Map(),
    });
    expect(markup).toContain("iCloud에 5개 있습니다 — 다시 불러오기를 누르세요");
    expect(markup).not.toContain("No transcripts for this day");
  });

  it("shows no iCloud hint when all of the day's entries are loaded", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-14", count: 1 }],
      entriesByDay: new Map([
        ["2026-05-14", [transcript({ recorded_at: "2026-05-14T01:02:03+09:00", text: "only one" })]],
      ]),
    });
    expect(markup).not.toContain("iCloud에");
    expect(markup).toContain("only one");
  });
});

describe("weekStartFor", () => {
  it("returns the Sunday of the week containing the given date", () => {
    expect(weekStartFor("2026-05-14")).toBe("2026-05-10");
    expect(weekStartFor("2026-05-10")).toBe("2026-05-10");
    expect(weekStartFor("2026-05-16")).toBe("2026-05-10");
  });
});

describe("addDays", () => {
  it("shifts a YYYY-MM-DD day, crossing month boundaries", () => {
    expect(addDays("2026-05-10", 7)).toBe("2026-05-17");
    expect(addDays("2026-05-10", -7)).toBe("2026-05-03");
    expect(addDays("2026-05-31", 1)).toBe("2026-06-01");
  });
});

describe("previousWeekDisabledReason", () => {
  it("returns a reason at or before the earliest week", () => {
    expect(previousWeekDisabledReason("2026-05-10", "2026-05-13")).toBe("Already on the earliest week");
    expect(previousWeekDisabledReason("2026-05-03", "2026-05-13")).toBe("Already on the earliest week");
  });
  it("returns null when there is an older week", () => {
    expect(previousWeekDisabledReason("2026-05-10", "2026-05-01")).toBeNull();
  });
});

describe("nextWeekDisabledReason", () => {
  it("returns a reason at or after today's week", () => {
    expect(nextWeekDisabledReason("2026-05-10", "2026-05-14")).toBe("Already on this week");
    expect(nextWeekDisabledReason("2026-05-17", "2026-05-14")).toBe("Already on this week");
  });
  it("returns null when there is a newer week", () => {
    expect(nextWeekDisabledReason("2026-05-03", "2026-05-14")).toBeNull();
  });
});
