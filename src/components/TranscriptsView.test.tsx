import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type {
  LlmGenerateResponse,
  LlmProgressEvent,
  RecentTranscript,
  TranscriptDay,
} from "../lib/types";
import {
  TranscriptsView,
  addDays,
  filterTranscriptsBySource,
  nextPlayableTranscriptPath,
  nextWeekDisabledReason,
  previousWeekDisabledReason,
  selectTranscriptPathsAfterShiftClick,
  selectTranscriptPathsForEntries,
  shouldCopySelectedTranscriptsOnKeydown,
  transcriptPlaybackStateAfterToggle,
  transcriptRegenerationTargetsForContextMenu,
  transcriptSourceFilterOptions,
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
  initialSourceFilter?: string;
  sourceLabels?: Record<string, string>;
  models?: Array<{ id: string; display_name: string; status: "ready" | "missing" }>;
  selectedModelId?: string;
  onRegenerate?: (entries: readonly RecentTranscript[], modelId?: string) => void;
  onGenerateReport?: (entries: readonly RecentTranscript[], kind: "summary" | "detailed_report") => void;
  openrouterKeyConfigured?: boolean;
  reportGenerating?: boolean;
  reportProgress?: LlmProgressEvent[];
  reportResult?: LlmGenerateResponse | null;
  onDownloadReport?: (reportId: string, fileName: string) => void;
  onOpenFolder?: (entry: RecentTranscript) => void;
}) {
  return renderToStaticMarkup(
    <TranscriptsView
      days={props.days ?? []}
      entriesByDay={props.entriesByDay ?? new Map()}
      loadingDay={props.loadingDay ?? null}
      initialPlayingTranscriptPath={props.initialPlayingTranscriptPath}
      initialSourceFilter={props.initialSourceFilter}
      sourceLabels={props.sourceLabels}
      models={props.models}
      selectedModelId={props.selectedModelId}
      onRegenerate={props.onRegenerate}
      onGenerateReport={props.onGenerateReport}
      openrouterKeyConfigured={props.openrouterKeyConfigured}
      reportGenerating={props.reportGenerating}
      reportProgress={props.reportProgress}
      reportResult={props.reportResult}
      onDownloadReport={props.onDownloadReport}
      onOpenFolder={props.onOpenFolder}
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

  it("renders week-day cells in Sun-Mon-Tue-Wed-Thu-Fri-Sat order with weekend tone hooks", () => {
    const markup = view({ today: new Date("2026-05-14T12:00:00+09:00") });
    const labels = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
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

  it("marks playable transcript rows as regeneration context-menu targets", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "regeneratable transcript" })]]]),
      models: [{ id: "whisper-medium", display_name: "Whisper Medium", status: "ready" }],
      onRegenerate: () => undefined,
    });

    expect(markup).toContain('data-regenerate-available="true"');
    expect(markup).toContain('data-audio-path="/tmp/WakeNote/20260510/010203.m4a"');
  });

  it("renders source badges inline between timestamp and transcript text", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 3 }],
      entriesByDay: new Map([
        ["2026-05-10", [
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            source: "microphone",
            source_label: null,
            text: "mic transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010204-youtube.txt",
            recorded_at: "2026-05-10T01:02:04+09:00",
            source: "system",
            source_label: "youtube",
            text: "youtube transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010205-meet.txt",
            recorded_at: "2026-05-10T01:02:05+09:00",
            source: "system",
            source_label: "meet",
            text: "meet transcript",
          }),
        ]],
      ]),
    });

    expect(markup).toContain("Mic");
    expect(markup).toContain("YouTube");
    expect(markup).toContain("Meet");
    expect(markup).toContain("transcript-source-badge--youtube");
    expect(markup).toContain("transcript-source-badge--meet");
    expect(markup).not.toContain("transcript-entry__meta");

    const youtubeRowStart = markup.indexOf("010204-youtube.txt");
    const youtubeBadge = markup.indexOf("YouTube", youtubeRowStart);
    const youtubeText = markup.indexOf("youtube transcript", youtubeRowStart);
    expect(youtubeRowStart).toBeGreaterThan(-1);
    expect(youtubeBadge).toBeGreaterThan(youtubeRowStart);
    expect(youtubeBadge).toBeLessThan(youtubeText);
  });

  it("renders custom source labels from settings for system transcripts", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      sourceLabels: { "custom-source-2": "Spotify" },
      entriesByDay: new Map([
        ["2026-05-10", [
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010206-custom-source-2.txt",
            recorded_at: "2026-05-10T01:02:06+09:00",
            source: "system",
            source_label: "custom-source-2",
            text: "spotify transcript",
          }),
        ]],
      ]),
    });

    expect(markup).toContain("Spotify");
    expect(markup).not.toContain("Custom Source 2");
  });

  it("filters the visible transcript rows by source", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 3 }],
      initialSourceFilter: "system:youtube",
      entriesByDay: new Map([
        ["2026-05-10", [
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010203.txt",
            source: "microphone",
            source_label: null,
            text: "mic transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010204-youtube.txt",
            recorded_at: "2026-05-10T01:02:04+09:00",
            source: "system",
            source_label: "youtube",
            text: "youtube transcript",
          }),
          transcript({
            transcript_path: "/tmp/WakeNote/20260510/010205-meet.txt",
            recorded_at: "2026-05-10T01:02:05+09:00",
            source: "system",
            source_label: "meet",
            text: "meet transcript",
          }),
        ]],
      ]),
    });

    expect(markup).toContain("Source");
    expect(markup).toContain("YouTube (1)");
    expect(markup).toContain("1 / 3 transcripts");
    expect(markup).toContain("youtube transcript");
    expect(markup).not.toContain("mic transcript");
    expect(markup).not.toContain("meet transcript");
  });

  it("uses the filtered rows when selecting every visible transcript", () => {
    const entries = [
      transcript({
        transcript_path: "/tmp/WakeNote/20260510/010203.txt",
        source: "microphone",
      }),
      transcript({
        transcript_path: "/tmp/WakeNote/20260510/010204-youtube.txt",
        source: "system",
        source_label: "youtube",
      }),
      transcript({
        transcript_path: "/tmp/WakeNote/20260510/010205-youtube.txt",
        source: "system",
        source_label: "youtube",
      }),
    ];

    const visible = filterTranscriptsBySource(entries, "system:youtube");
    const selected = selectTranscriptPathsForEntries(visible);

    expect([...selected]).toEqual([
      "/tmp/WakeNote/20260510/010204-youtube.txt",
      "/tmp/WakeNote/20260510/010205-youtube.txt",
    ]);
  });

  it("selects every visible transcript between the anchor and shift-clicked row", () => {
    const entries = [
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010203.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010204.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010205.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010206.txt" }),
    ];

    const selected = selectTranscriptPathsAfterShiftClick(
      entries,
      new Set(["/tmp/WakeNote/20260510/010203.txt"]),
      "/tmp/WakeNote/20260510/010203.txt",
      "/tmp/WakeNote/20260510/010206.txt",
    );

    expect([...selected]).toEqual([
      "/tmp/WakeNote/20260510/010203.txt",
      "/tmp/WakeNote/20260510/010204.txt",
      "/tmp/WakeNote/20260510/010205.txt",
      "/tmp/WakeNote/20260510/010206.txt",
    ]);
  });

  it("falls back to the clicked row when shift-click has no visible anchor", () => {
    const entries = [
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010203.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010204.txt" }),
    ];

    expect([
      ...selectTranscriptPathsAfterShiftClick(
        entries,
        new Set(),
        "/tmp/WakeNote/20260510/missing.txt",
        "/tmp/WakeNote/20260510/010204.txt",
      ),
    ]).toEqual(["/tmp/WakeNote/20260510/010204.txt"]);
  });

  it("uses the selected rows as regeneration context targets when right-clicking selection", () => {
    const entries = [
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010203.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010204.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010205.txt" }),
    ];

    const targets = transcriptRegenerationTargetsForContextMenu(
      entries[1],
      entries,
      new Set([
        "/tmp/WakeNote/20260510/010203.txt",
        "/tmp/WakeNote/20260510/010204.txt",
      ]),
    );

    expect(targets.map((entry) => entry.transcript_path)).toEqual([
      "/tmp/WakeNote/20260510/010203.txt",
      "/tmp/WakeNote/20260510/010204.txt",
    ]);
  });

  it("uses only the right-clicked row as regeneration target outside the selection", () => {
    const entries = [
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010203.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010204.txt" }),
    ];

    const targets = transcriptRegenerationTargetsForContextMenu(
      entries[1],
      entries,
      new Set(["/tmp/WakeNote/20260510/010203.txt"]),
    );

    expect(targets.map((entry) => entry.transcript_path)).toEqual([
      "/tmp/WakeNote/20260510/010204.txt",
    ]);
  });

  it("handles command-copy only while transcript rows are selected", () => {
    expect(
      shouldCopySelectedTranscriptsOnKeydown(
        { key: "c", metaKey: true, ctrlKey: false, altKey: false },
        2,
      ),
    ).toBe(true);
    expect(
      shouldCopySelectedTranscriptsOnKeydown(
        { key: "C", metaKey: false, ctrlKey: true, altKey: false },
        1,
      ),
    ).toBe(true);
    expect(
      shouldCopySelectedTranscriptsOnKeydown(
        { key: "c", metaKey: true, ctrlKey: false, altKey: false },
        0,
      ),
    ).toBe(false);
    expect(
      shouldCopySelectedTranscriptsOnKeydown(
        { key: "v", metaKey: true, ctrlKey: false, altKey: false },
        1,
      ),
    ).toBe(false);
  });

  it("builds source filter options from microphone, built-in, and custom sources", () => {
    const options = transcriptSourceFilterOptions(
      [
        transcript({ source: "microphone", source_label: null }),
        transcript({
          transcript_path: "/tmp/WakeNote/20260510/010204-youtube.txt",
          source: "system",
          source_label: "youtube",
        }),
        transcript({
          transcript_path: "/tmp/WakeNote/20260510/010205-custom-source-2.txt",
          source: "system",
          source_label: "custom-source-2",
        }),
      ],
      { "custom-source-2": "Spotify" },
    );

    expect(options).toEqual([
      { id: "all", label: "All sources", count: 3 },
      { id: "microphone", label: "Mic", count: 1 },
      { id: "system:youtube", label: "YouTube", count: 1 },
      { id: "system:custom-source-2", label: "Spotify", count: 1 },
    ]);
  });

  it("shows pause and folder controls for the active playable row", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "playable transcript" })]]]),
      initialPlayingTranscriptPath: "/tmp/WakeNote/20260510/010203.txt",
      onOpenFolder: () => undefined,
    });

    expect(markup).toContain('aria-label="Pause recording from 2026-05-10 01:02:03"');
    expect(markup).toContain('aria-label="Open recording folder for 2026-05-10 01:02:03"');
  });

  it("starts a different playable transcript immediately instead of pausing current playback", () => {
    expect(
      transcriptPlaybackStateAfterToggle(
        { playingTranscriptPath: "/tmp/WakeNote/20260510/010203.txt", playbackPaused: false },
        transcript({
          transcript_path: "/tmp/WakeNote/20260510/020304.txt",
          audio_path: "/tmp/WakeNote/20260510/020304.m4a",
        }),
      ),
    ).toEqual({
      playingTranscriptPath: "/tmp/WakeNote/20260510/020304.txt",
      playbackPaused: false,
      shouldChangeActiveTranscript: true,
    });
  });

  it("keeps regeneration controls out of the inline transcript list", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "regeneratable transcript" })]]]),
      models: [
        { id: "whisper-medium", display_name: "Whisper Medium", status: "ready" },
        { id: "whisper-small", display_name: "Whisper Small", status: "ready" },
        { id: "whisper-large", display_name: "Whisper Large", status: "missing" },
      ],
      selectedModelId: "whisper-medium",
      onRegenerate: () => undefined,
    });

    expect(markup).not.toContain("transcript-regenerate-model");
    expect(markup).not.toContain("transcript-entry__regenerate");
    expect(markup).not.toContain("Regenerate with");
    expect(markup).not.toContain("Whisper Medium");
    expect(markup).not.toContain("Whisper Large");
  });

  it("always renders a reload button and shows copy actions only when the day has entries", () => {
    const withEntries = view({
      today: new Date("2026-05-19T18:00:00+09:00"),
      days: [{ day: "2026-05-19", count: 1 }],
      entriesByDay: new Map([["2026-05-19", [transcript({ recorded_at: "2026-05-19T15:53:23+09:00", text: "슬립~" })]]]),
    });
    expect(withEntries).toContain('aria-label="Reload this day"');
    expect(withEntries).toContain('aria-label="Copy all transcripts for this day"');
    expect(withEntries).toContain("Copy all");

    const empty = view({ today: new Date("2026-05-19T18:00:00+09:00") });
    expect(empty).toContain('aria-label="Reload this day"');
    expect(empty).not.toContain("Copy all");
  });

  it("shows OpenRouter summary and detailed report actions for visible transcripts", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "reportable transcript" })]]]),
      openrouterKeyConfigured: true,
      onGenerateReport: () => undefined,
    });

    expect(markup).toContain('aria-label="Summarize all visible transcripts for this day"');
    expect(markup).toContain('aria-label="Create detailed report from all visible transcripts for this day"');
    expect(markup).toContain("Summary all");
    expect(markup).toContain("Report all");
  });

  it("disables OpenRouter report actions until an API key is saved", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "reportable transcript" })]]]),
      openrouterKeyConfigured: false,
      onGenerateReport: () => undefined,
    });

    expect(markup).toMatch(/aria-label="Summarize all visible transcripts for this day"[^>]*disabled=""/);
    expect(markup).toContain("Save an OpenRouter API key in Advanced settings first");
  });

  it("shows live agent stages and quality feedback while a report is running", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      reportGenerating: true,
      reportProgress: [
        {
          run_id: "report-run-1",
          stage: "generating",
          iteration: 1,
          max_iterations: 5,
          message: "Drafting report",
          detail: null,
        },
        {
          run_id: "report-run-1",
          stage: "evaluating",
          iteration: 1,
          max_iterations: 5,
          message: "Checking success criteria",
          detail: "Action items are missing.",
        },
      ],
    });

    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain("Drafting report");
    expect(markup).toContain("Checking success criteria");
    expect(markup).toContain("Action items are missing.");
    expect(markup).toContain("Iteration 1 of 5");
  });

  it("distinguishes success from reaching the maximum iteration limit", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      reportProgress: [
        {
          run_id: "report-run-1",
          stage: "max_iterations_reached",
          iteration: 3,
          max_iterations: 3,
          message: "Maximum iterations reached; saved the latest draft",
          detail: "Evidence notes are still incomplete.",
        },
      ],
      reportResult: {
        run_id: "report-run-1",
        content: "best available report",
        iterations_used: 3,
        max_iterations: 3,
        success_criteria_met: false,
        completion_reason: "max_iterations_reached",
        quality_feedback: "Evidence notes are still incomplete.",
        model: "z-ai/glm-5.2",
        report_id: "20260713-100000-detailed-report",
        usage: {
          request_count: 6,
          prompt_tokens: 1200,
          completion_tokens: 400,
          total_tokens: 1600,
          cost: 0.0125,
        },
        report_path: "/tmp/reports/report.md",
      },
      onDownloadReport: () => undefined,
    });

    expect(markup).toContain("Maximum iterations reached");
    expect(markup).toContain("3 of 3 iterations");
    expect(markup).toContain("Evidence notes are still incomplete.");
    expect(markup).not.toContain("Success criteria met");
    expect(markup).toContain('data-current-stage="max_iterations_reached"');
    expect(markup).toContain('aria-label="Download generated report as Markdown"');
  });

  it("disables the reload button while the active day is loading", () => {
    const markup = view({
      today: new Date("2026-05-19T18:00:00+09:00"),
      loadingDay: "2026-05-19",
    });
    expect(markup).toMatch(/<button[^>]*aria-label="Reload this day"[^>]*disabled=""/);
    expect(markup).toContain("Loading…");
  });

  it("keeps the selected transcript player inside the archive flow", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [transcript({ text: "currently playing transcript" })]]]),
      initialPlayingTranscriptPath: "/tmp/WakeNote/20260510/010203.txt",
    });
    expect(markup).toContain('data-slot="transcript-player-dock"');
    expect(markup).toContain('data-slot="transcript-toolbar"');
    expect(markup).toContain("Autoplay next");
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
    expect(markup).toContain("2 more in iCloud");
    expect(markup).toContain("local one");
    expect(markup).toContain('aria-label="Reload this day"');
  });

  it("prompts a reload in the empty state when a day is entirely iCloud-evicted", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-14", count: 5 }],
      entriesByDay: new Map(),
    });
    expect(markup).toContain("5 in iCloud — press Reload");
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
    expect(markup).not.toContain("in iCloud");
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

describe("nextPlayableTranscriptPath", () => {
  const entries: RecentTranscript[] = [
    transcript({ transcript_path: "/a.txt", audio_path: "/a.m4a" }),
    transcript({ transcript_path: "/b.txt", audio_path: null }),
    transcript({ transcript_path: "/c.txt", audio_path: "/c.m4a" }),
  ];

  it("returns the next entry with audio after the current path", () => {
    expect(nextPlayableTranscriptPath(entries, "/a.txt")).toBe("/c.txt");
  });

  it("skips entries that have no audio", () => {
    // /b has no audio, so advancing from /a lands on /c, not /b.
    expect(nextPlayableTranscriptPath(entries, "/a.txt")).toBe("/c.txt");
  });

  it("returns null at the end of the list", () => {
    expect(nextPlayableTranscriptPath(entries, "/c.txt")).toBeNull();
  });

  it("returns null when no later entry has audio", () => {
    const trailing: RecentTranscript[] = [
      transcript({ transcript_path: "/a.txt", audio_path: "/a.m4a" }),
      transcript({ transcript_path: "/b.txt", audio_path: null }),
    ];
    expect(nextPlayableTranscriptPath(trailing, "/a.txt")).toBeNull();
  });

  it("returns null when the current path is absent or null", () => {
    expect(nextPlayableTranscriptPath(entries, "/missing.txt")).toBeNull();
    expect(nextPlayableTranscriptPath(entries, null)).toBeNull();
  });
});
