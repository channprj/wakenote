import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type {
  LlmReportRunSnapshot,
  RecentTranscript,
  TranscriptDay,
} from "../lib/types";
import type { AudioMergeUiState } from "../lib/audio-merge";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
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
  transcriptVisibilityTargetsForContextMenu,
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

function reportRun(
  overrides: Partial<LlmReportRunSnapshot> = {},
): LlmReportRunSnapshot {
  const run: LlmReportRunSnapshot = {
    run_id: "run-transcripts-1",
    parent_run_id: null,
    revision: 2,
    status: "running",
    stage: "generating",
    kind: "summary",
    created_at: "2026-07-18T00:00:00Z",
    updated_at: "2026-07-18T00:00:01Z",
    started_at: "2026-07-18T00:00:01Z",
    finished_at: null,
    iteration: 1,
    max_iterations: 3,
    message: "Drafting report",
    detail: null,
    error: null,
    progress: [],
    model: "z-ai/glm-5.2",
    selected_count: 1,
    date_range: "2026-07-18",
    report_id: null,
    report_path: null,
    completion_reason: null,
    success_criteria_met: null,
    quality_feedback: null,
    usage: null,
    ...overrides,
  };
  return {
    ...run,
    progress: overrides.progress ?? [
      {
        run_id: run.run_id,
        stage: run.stage ?? "preparing",
        iteration: run.iteration,
        max_iterations: run.max_iterations,
        message: run.message,
        detail: run.detail,
      },
    ],
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
  models?: Array<{
    id: string;
    display_name: string;
    status: "ready" | "missing";
  }>;
  selectedModelId?: string;
  onRegenerate?: (
    entries: readonly RecentTranscript[],
    modelId?: string,
  ) => void;
  onGenerateReport?: (
    entries: readonly RecentTranscript[],
    kind: "summary" | "detailed_report",
  ) => void;
  openrouterKeyConfigured?: boolean;
  reportRun?: LlmReportRunSnapshot | null;
  onOpenReports?: () => void;
  onOpenFolder?: (entry: RecentTranscript) => void;
  audioMergeState?: AudioMergeUiState;
  onMergeAudio?: (entries: readonly RecentTranscript[]) => void;
  onOpenMergedAudioFolder?: (destinationPath: string) => void;
  visibilityMode?: "visible" | "hidden";
  visibleCountByDay?: Map<string, number>;
  hiddenCountByDay?: Map<string, number>;
  visibilityMutating?: boolean;
  visibilityStatus?: string;
  visibilityError?: string | null;
  onVisibilityModeChange?: (mode: "visible" | "hidden") => void;
  onSetTranscriptsHidden?: (
    entries: readonly RecentTranscript[],
    hidden: boolean,
  ) => boolean | Promise<boolean>;
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
      reportRun={props.reportRun}
      onOpenReports={props.onOpenReports}
      onOpenFolder={props.onOpenFolder}
      audioMergeState={props.audioMergeState}
      onMergeAudio={props.onMergeAudio}
      onOpenMergedAudioFolder={props.onOpenMergedAudioFolder}
      visibilityMode={props.visibilityMode}
      visibleCountByDay={props.visibleCountByDay}
      hiddenCountByDay={props.hiddenCountByDay}
      visibilityMutating={props.visibilityMutating}
      visibilityStatus={props.visibilityStatus}
      visibilityError={props.visibilityError}
      onVisibilityModeChange={props.onVisibilityModeChange}
      onSetTranscriptsHidden={props.onSetTranscriptsHidden}
      today={props.today}
    />,
  );
}

describe("TranscriptsView audio merge status", () => {
  const entriesByDay = new Map([
    [
      "2026-05-10",
      [
        transcript({ text: "First audio" }),
        transcript({
          transcript_path: "/tmp/WakeNote/20260510/010204.txt",
          audio_path: "/tmp/WakeNote/20260510/010204.wav",
          recorded_at: "2026-05-10T01:02:04+09:00",
          text: "Second audio",
        }),
      ],
    ],
  ]);

  it("renders the audio merge status for running, saved, and error states", () => {
    const cases: Array<{
      audioMergeState: AudioMergeUiState;
      onOpenMergedAudioFolder?: (destinationPath: string) => void;
      expected: string[];
    }> = [
      {
        audioMergeState: {
          status: "running",
          operation_id: "merge-current",
          stage: "combining",
          completed_inputs: 1,
          total_inputs: 2,
          percent: 55,
          current_input_label: "010203.m4a",
        },
        expected: [
          "Merging audio · 55%",
          "1 of 2 files",
          'aria-label="Audio merge progress"',
        ],
      },
      {
        audioMergeState: {
          status: "saved",
          operation_id: "merge-current",
          destination_path: "/tmp/WakeNote merged.m4a",
          input_count: 2,
        },
        onOpenMergedAudioFolder: () => {},
        expected: [
          "Audio merged · 2 files",
          "/tmp/WakeNote merged.m4a",
          'aria-label="Open merged audio folder"',
        ],
      },
      {
        audioMergeState: {
          status: "error",
          operation_id: "merge-current",
          message: "Could not encode audio",
        },
        expected: [
          "Audio merge failed",
          "Could not encode audio",
          "Selection preserved. Try again.",
        ],
      },
    ];

    for (const { audioMergeState, onOpenMergedAudioFolder, expected } of cases) {
      const markup = view({
        today: new Date("2026-05-10T12:00:00+09:00"),
        entriesByDay,
        audioMergeState,
        onOpenMergedAudioFolder,
      });

      for (const text of expected) {
        expect(markup, `${audioMergeState.status}: ${text}`).toContain(text);
      }
    }
  });
});

describe("TranscriptsView", () => {
  it("shows today's date page by default with weekly calendar pagination", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [
        { day: "2026-05-13", count: 1 },
        { day: "2026-05-14", count: 1 },
      ],
      entriesByDay: new Map([
        [
          "2026-05-13",
          [
            transcript({
              recorded_at: "2026-05-13T01:02:03+09:00",
              text: "yesterday transcript",
            }),
          ],
        ],
        [
          "2026-05-14",
          [
            transcript({
              recorded_at: "2026-05-14T01:02:03+09:00",
              text: "today transcript",
            }),
          ],
        ],
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
      expect(next, `expected ${label} after previous label`).toBeGreaterThan(
        cursor,
      );
      cursor = next;
    }
    expect(markup).toContain('data-day-of-week="0"');
    expect(markup).toContain('data-day-of-week="6"');
  });

  it("disables future days within the current week and the Next-week arrow", () => {
    const markup = view({ today: new Date("2026-05-14T12:00:00+09:00") });
    expect(markup).toMatch(
      /<button[^>]*aria-label="Go to 2026-05-15 transcripts"[^>]*disabled=""/,
    );
    expect(markup).toMatch(
      /<button[^>]*aria-label="Go to 2026-05-16 transcripts"[^>]*disabled=""/,
    );
    expect(markup).toMatch(
      /<button[^>]*aria-label="Next week"[^>]*disabled=""[^>]*title="Already on this week"/,
    );
  });

  it("disables the Previous-week arrow on the earliest week with transcripts", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-13", count: 1 }],
    });
    expect(markup).toMatch(
      /<button[^>]*aria-label="Previous week"[^>]*disabled=""[^>]*title="Already on the earliest week"/,
    );
  });

  it("starts on today's empty date instead of the newest saved day", () => {
    const markup = view({
      today: new Date("2026-05-14T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([
        ["2026-05-10", [transcript({ text: "older saved transcript" })]],
      ]),
    });
    expect(markup).toContain("2026-05-14");
    expect(markup).toContain("No transcripts for this day");
    expect(markup).not.toContain("older saved transcript");
    expect(markup).toContain('aria-label="Go to 2026-05-10 transcripts"');
  });

  it("renders transcripts newest-first within the selected date by default", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 2 }],
      entriesByDay: new Map([
        [
          "2026-05-10",
          [
            transcript({
              transcript_path: "/tmp/WakeNote/20260510/180000.txt",
              recorded_at: "2026-05-10T18:00:00+09:00",
              text: "evening transcript",
            }),
            transcript({
              transcript_path: "/tmp/WakeNote/20260510/090000.txt",
              recorded_at: "2026-05-10T09:00:00+09:00",
              text: "morning transcript",
            }),
          ],
        ],
      ]),
    });
    expect(markup.indexOf("evening transcript")).toBeLessThan(
      markup.indexOf("morning transcript"),
    );
  });

  it("shows Visible and Hidden counts without treating hidden rows as iCloud gaps", () => {
    const day = "2026-05-10";
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day, count: 2 }],
      entriesByDay: new Map([
        [day, [transcript({ text: "visible transcript" })]],
      ]),
      visibleCountByDay: new Map([[day, 1]]),
      hiddenCountByDay: new Map([[day, 1]]),
      onSetTranscriptsHidden: () => true,
    });

    expect(markup).toContain("Visible");
    expect(markup).toContain("Hidden");
    expect(markup).toContain(
      'aria-label="Select transcript 2026-05-10 01:02:03"',
    );
    expect(markup).not.toContain("more in iCloud");
  });

  it("explains that an empty Hidden day preserves files", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      visibilityMode: "hidden",
      hiddenCountByDay: new Map([["2026-05-10", 0]]),
    });

    expect(markup).toContain(
      "No hidden transcripts for this day · Files remain on disk",
    );
    expect(markup).not.toContain("Restore selected");
  });

  it("preserves long transcript text and paths in the compact row structure", () => {
    const entry = transcript({
      transcript_path: LONG_CONTENT.path.replace(/\.m4a$/, ".txt"),
      audio_path: LONG_CONTENT.path,
      text: LONG_CONTENT.korean,
    });
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([["2026-05-10", [entry]]]),
    });

    expect(markup).toContain(LONG_CONTENT.korean);
    expect(markup).toContain(`title="${entry.transcript_path}"`);
    expect(markup).toContain("transcript-entry__actions");
  });

  it("renders a play button and inline file:// timestamp link for each transcript", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([
        ["2026-05-10", [transcript({ text: "playable transcript" })]],
      ]),
    });
    expect(markup).toContain("playable transcript");
    expect(markup).toContain(
      'aria-label="Play recording from 2026-05-10 01:02:03"',
    );
    expect(markup).toContain("transcript-entry__play");
    expect(markup).toContain(
      '<a class="transcript-entry__timestamp" href="file:///tmp/WakeNote/20260510/010203.txt" title="/tmp/WakeNote/20260510/010203.txt"><span>2026-05-10 01:02:03</span></a>',
    );
  });

  it("marks playable transcript rows as regeneration context-menu targets", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([
        ["2026-05-10", [transcript({ text: "regeneratable transcript" })]],
      ]),
      models: [
        {
          id: "whisper-medium",
          display_name: "Whisper Medium",
          status: "ready",
        },
      ],
      onRegenerate: () => undefined,
    });

    expect(markup).toContain('data-regenerate-available="true"');
    expect(markup).toContain(
      'data-audio-path="/tmp/WakeNote/20260510/010203.m4a"',
    );
  });

  it("renders source badges inline between timestamp and transcript text", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 5 }],
      entriesByDay: new Map([
        [
          "2026-05-10",
          [
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
            transcript({
              transcript_path: "/tmp/WakeNote/20260510/010206-mic-merged.txt",
              recorded_at: "2026-05-10T01:02:06+09:00",
              source: "microphone",
              source_label: "mic-merged",
              device_id: "input-1-boya-cm40+input-2-by-v",
              device_name: "BOYA CM40 + BY-V",
              microphone_slot: null,
              text: "merged transcript",
            }),
            transcript({
              transcript_path: "/tmp/WakeNote/20260510/010207-dictation.txt",
              recorded_at: "2026-05-10T01:02:07+09:00",
              source: "microphone",
              source_label: "dictation",
              device_id: "input-1-boya-cm40",
              device_name: "BOYA CM40",
              microphone_slot: null,
              text: "dictation transcript",
            }),
          ],
        ],
      ]),
    });

    expect(markup).toContain("Mic");
    expect(markup).toContain("YouTube");
    expect(markup).toContain("Meet");
    expect(markup).toContain(">Merged Mic</span>");
    expect(markup).toContain('title="Merged Mic · BOYA CM40 + BY-V"');
    expect(markup).toContain(">Dict</span>");
    expect(markup).toContain('title="Dictation transcript"');
    expect(markup).toContain("transcript-source-badge--dictation");
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
        [
          "2026-05-10",
          [
            transcript({
              transcript_path:
                "/tmp/WakeNote/20260510/010206-custom-source-2.txt",
              recorded_at: "2026-05-10T01:02:06+09:00",
              source: "system",
              source_label: "custom-source-2",
              text: "spotify transcript",
            }),
          ],
        ],
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
        [
          "2026-05-10",
          [
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
          ],
        ],
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

  it("extends shift-click selection from a visible anchor or falls back to the clicked row", () => {
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

    expect([...selected], "visible anchor").toEqual([
      "/tmp/WakeNote/20260510/010203.txt",
      "/tmp/WakeNote/20260510/010204.txt",
      "/tmp/WakeNote/20260510/010205.txt",
      "/tmp/WakeNote/20260510/010206.txt",
    ]);

    const withoutAnchor = [
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010203.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010204.txt" }),
    ];

    expect(
      [
        ...selectTranscriptPathsAfterShiftClick(
          withoutAnchor,
          new Set(),
          "/tmp/WakeNote/20260510/missing.txt",
          "/tmp/WakeNote/20260510/010204.txt",
        ),
      ],
      "missing anchor",
    ).toEqual(["/tmp/WakeNote/20260510/010204.txt"]);
  });

  it("targets the selection only when right-clicking a selected row", () => {
    const entries = [
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010203.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010204.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010205.txt" }),
    ];

    const insideSelection = transcriptRegenerationTargetsForContextMenu(
      entries[1],
      entries,
      new Set([
        "/tmp/WakeNote/20260510/010203.txt",
        "/tmp/WakeNote/20260510/010204.txt",
      ]),
    );

    expect(
      insideSelection.map((entry) => entry.transcript_path),
      "right-click inside the selection",
    ).toEqual([
      "/tmp/WakeNote/20260510/010203.txt",
      "/tmp/WakeNote/20260510/010204.txt",
    ]);

    const outsideEntries = [
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010203.txt" }),
      transcript({ transcript_path: "/tmp/WakeNote/20260510/010204.txt" }),
    ];

    const outsideSelection = transcriptRegenerationTargetsForContextMenu(
      outsideEntries[1],
      outsideEntries,
      new Set(["/tmp/WakeNote/20260510/010203.txt"]),
    );

    expect(
      outsideSelection.map((entry) => entry.transcript_path),
      "right-click outside the selection",
    ).toEqual(["/tmp/WakeNote/20260510/010204.txt"]);
  });

  it("keeps every selected transcript as a context visibility target", () => {
    const entries = [
      transcript({
        transcript_path: "/tmp/WakeNote/20260510/010203.txt",
      }),
      transcript({
        transcript_path: "/tmp/WakeNote/20260510/010204.txt",
        audio_path: null,
      }),
    ];

    expect(
      transcriptVisibilityTargetsForContextMenu(
        entries[0],
        entries,
        new Set(entries.map((entry) => entry.transcript_path)),
      ),
    ).toEqual(entries);
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
        transcript({
          source: "microphone",
          source_label: "mic-primary-wired",
          device_id: "input-1-wired",
          device_name: "Wired",
          microphone_slot: "primary",
        }),
        transcript({
          transcript_path:
            "/tmp/WakeNote/20260510/010203-mic-secondary-wireless.txt",
          source: "microphone",
          source_label: "mic-secondary-wireless",
          device_id: "input-2-wireless",
          device_name: "Wireless",
          microphone_slot: "secondary",
        }),
        transcript({
          transcript_path: "/tmp/WakeNote/20260510/010203-mic-merged.txt",
          source: "microphone",
          source_label: "mic-merged",
          device_id: "input-1-boya-cm40+input-2-by-v",
          device_name: "BOYA CM40 + BY-V",
          microphone_slot: null,
        }),
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
      { id: "all", label: "All sources", count: 5 },
      { id: "dictation", label: "Dictations", count: 0 },
      { id: "microphone:input-1-wired", label: "Wired", count: 1 },
      { id: "microphone:input-2-wireless", label: "Wireless", count: 1 },
      {
        id: "microphone:input-1-boya-cm40+input-2-by-v",
        label: "Merged Mic",
        count: 1,
      },
      { id: "system:youtube", label: "YouTube", count: 1 },
      { id: "system:custom-source-2", label: "Spotify", count: 1 },
    ]);
  });

  it("keeps Dictation in its own source filter instead of its physical microphone", () => {
    const microphone = transcript({
      source: "microphone",
      source_label: "mic-primary-wired",
      device_id: "input-1-wired",
      device_name: "Wired",
    });
    const dictation = transcript({
      transcript_path: "/tmp/WakeNote/20260510/010204-dictation.txt",
      source: "microphone",
      source_label: "dictation",
      device_id: "input-1-wired",
      device_name: "Wired",
    });

    expect(transcriptSourceFilterOptions([microphone, dictation])).toEqual([
      { id: "all", label: "All sources", count: 2 },
      { id: "dictation", label: "Dictations", count: 1 },
      { id: "microphone:input-1-wired", label: "Wired", count: 1 },
    ]);
    expect(
      filterTranscriptsBySource([microphone, dictation], "dictation"),
    ).toEqual([dictation]);
  });

  it("shows pause and folder controls for the active playable row", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([
        ["2026-05-10", [transcript({ text: "playable transcript" })]],
      ]),
      initialPlayingTranscriptPath: "/tmp/WakeNote/20260510/010203.txt",
      onOpenFolder: () => undefined,
    });

    expect(markup).toContain(
      'aria-label="Pause recording from 2026-05-10 01:02:03"',
    );
    expect(markup).toContain(
      'aria-label="Open recording folder for 2026-05-10 01:02:03"',
    );
  });

  it("starts a different playable transcript immediately instead of pausing current playback", () => {
    expect(
      transcriptPlaybackStateAfterToggle(
        {
          playingTranscriptPath: "/tmp/WakeNote/20260510/010203.txt",
          playbackPaused: false,
        },
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
      entriesByDay: new Map([
        ["2026-05-10", [transcript({ text: "regeneratable transcript" })]],
      ]),
      models: [
        {
          id: "whisper-medium",
          display_name: "Whisper Medium",
          status: "ready",
        },
        { id: "whisper-small", display_name: "Whisper Small", status: "ready" },
        {
          id: "whisper-large",
          display_name: "Whisper Large",
          status: "missing",
        },
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

  it("keeps day-wide actions behind More in every toolbar state", () => {
    const reportableEntries = new Map([
      ["2026-05-10", [transcript({ text: "reportable transcript" })]],
    ]);
    const cases: Array<{
      label: string;
      props: Parameters<typeof view>[0];
      contains?: string[];
      notContains?: string[];
    }> = [
      {
        label: "day with entries",
        props: {
          today: new Date("2026-05-19T18:00:00+09:00"),
          days: [{ day: "2026-05-19", count: 1 }],
          entriesByDay: new Map([
            [
              "2026-05-19",
              [
                transcript({
                  recorded_at: "2026-05-19T15:53:23+09:00",
                  text: "슬립~",
                }),
              ],
            ],
          ]),
        },
        notContains: ['aria-label="Copy all transcripts for this day"'],
      },
      {
        label: "empty day",
        props: { today: new Date("2026-05-19T18:00:00+09:00") },
        notContains: ["Copy all"],
      },
      {
        label: "OpenRouter key configured",
        props: {
          today: new Date("2026-05-10T12:00:00+09:00"),
          days: [{ day: "2026-05-10", count: 1 }],
          entriesByDay: reportableEntries,
          openrouterKeyConfigured: true,
          onGenerateReport: () => undefined,
        },
        notContains: ["Summary all", "Report all"],
      },
      {
        label: "no OpenRouter key",
        props: {
          today: new Date("2026-05-10T12:00:00+09:00"),
          days: [{ day: "2026-05-10", count: 1 }],
          entriesByDay: reportableEntries,
          openrouterKeyConfigured: false,
          onGenerateReport: () => undefined,
        },
        notContains: ["Save an OpenRouter API key"],
      },
      {
        label: "active report run",
        props: {
          today: new Date("2026-05-10T12:00:00+09:00"),
          days: [{ day: "2026-05-10", count: 1 }],
          entriesByDay: reportableEntries,
          openrouterKeyConfigured: true,
          reportRun: reportRun({ status: "running" }),
          onGenerateReport: () => undefined,
        },
        notContains: ["A report is already running"],
      },
      {
        label: "day loading",
        props: {
          today: new Date("2026-05-19T18:00:00+09:00"),
          loadingDay: "2026-05-19",
        },
        contains: ["Loading…"],
      },
    ];

    for (const { label, props, contains = [], notContains = [] } of cases) {
      const markup = view(props);

      expect(markup, `${label}: More trigger`).toContain(
        'aria-label="More day actions"',
      );
      for (const text of contains) {
        expect(markup, `${label}: ${text}`).toContain(text);
      }
      for (const text of notContains) {
        expect(markup, `${label}: ${text}`).not.toContain(text);
      }
    }
  });

  it("shows a durable run notice with Open Reports", () => {
    const run = reportRun({
      status: "running",
      stage: "generating",
      message: "Drafting report",
    });
    const markup = view({
      reportRun: run,
      onOpenReports: () => undefined,
    });

    expect(markup).toContain("Drafting report");
    expect(markup).toContain('aria-label="Open Reports"');
    expect(markup).not.toContain('aria-label="Report generation progress"');
  });

  it("keeps the selected transcript player inside the archive flow", () => {
    const markup = view({
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day: "2026-05-10", count: 1 }],
      entriesByDay: new Map([
        ["2026-05-10", [transcript({ text: "currently playing transcript" })]],
      ]),
      initialPlayingTranscriptPath: "/tmp/WakeNote/20260510/010203.txt",
    });
    expect(markup).toContain('data-slot="transcript-player-dock"');
    expect(markup).toContain('data-slot="transcript-toolbar"');
    expect(markup).toContain("Autoplay next");
    expect(markup).toContain("currently playing transcript");
    expect(markup).toContain('src="file:///tmp/WakeNote/20260510/010203.m4a"');
    expect(markup).toContain("Now playing");
  });

  it("derives the iCloud hint from the day count versus loaded entries", () => {
    const cases: Array<{
      count: number;
      loadedTexts: string[];
      contains: string[];
      notContains: string[];
    }> = [
      // 3 in the (size-based) count, 1 loaded → 2 still in iCloud.
      {
        count: 3,
        loadedTexts: ["local one"],
        contains: [
          "2 more in iCloud",
          "local one",
          'aria-label="More day actions"',
        ],
        notContains: [],
      },
      {
        count: 5,
        loadedTexts: [],
        contains: ["5 in iCloud — press Reload"],
        notContains: ["No transcripts for this day"],
      },
      {
        count: 1,
        loadedTexts: ["only one"],
        contains: ["only one"],
        notContains: ["in iCloud"],
      },
    ];

    for (const { count, loadedTexts, contains, notContains } of cases) {
      const label = `count=${count} loaded=${loadedTexts.length}`;
      const markup = view({
        today: new Date("2026-05-14T12:00:00+09:00"),
        days: [{ day: "2026-05-14", count }],
        entriesByDay:
          loadedTexts.length === 0
            ? new Map()
            : new Map([
                [
                  "2026-05-14",
                  loadedTexts.map((text) =>
                    transcript({
                      recorded_at: "2026-05-14T01:02:03+09:00",
                      text,
                    }),
                  ),
                ],
              ]),
      });

      for (const text of contains) {
        expect(markup, `${label}: ${text}`).toContain(text);
      }
      for (const text of notContains) {
        expect(markup, `${label}: ${text}`).not.toContain(text);
      }
    }
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

describe("week navigation disabled reasons", () => {
  it("derives Previous/Next week disabled reasons", () => {
    const cases: Array<
      [
        "previous" | "next",
        (weekStart: string, bound: string) => string | null,
        string,
        string,
        string | null,
      ]
    > = [
      [
        "previous",
        previousWeekDisabledReason,
        "2026-05-10",
        "2026-05-13",
        "Already on the earliest week",
      ],
      [
        "previous",
        previousWeekDisabledReason,
        "2026-05-03",
        "2026-05-13",
        "Already on the earliest week",
      ],
      ["previous", previousWeekDisabledReason, "2026-05-10", "2026-05-01", null],
      [
        "next",
        nextWeekDisabledReason,
        "2026-05-10",
        "2026-05-14",
        "Already on this week",
      ],
      [
        "next",
        nextWeekDisabledReason,
        "2026-05-17",
        "2026-05-14",
        "Already on this week",
      ],
      ["next", nextWeekDisabledReason, "2026-05-03", "2026-05-14", null],
    ];

    for (const [direction, reason, weekStart, bound, expected] of cases) {
      expect(
        reason(weekStart, bound),
        `${direction} week from ${weekStart} vs ${bound}`,
      ).toBe(expected);
    }
  });
});

describe("nextPlayableTranscriptPath", () => {
  const entries: RecentTranscript[] = [
    transcript({ transcript_path: "/a.txt", audio_path: "/a.m4a" }),
    transcript({ transcript_path: "/b.txt", audio_path: null }),
    transcript({ transcript_path: "/c.txt", audio_path: "/c.m4a" }),
  ];

  it("advances to the next entry with audio or returns null", () => {
    const trailing: RecentTranscript[] = [
      transcript({ transcript_path: "/a.txt", audio_path: "/a.m4a" }),
      transcript({ transcript_path: "/b.txt", audio_path: null }),
    ];
    const cases: Array<
      [string, RecentTranscript[], string | null, string | null]
    > = [
      // /b has no audio, so advancing from /a lands on /c, not /b.
      ["skips entries without audio", entries, "/a.txt", "/c.txt"],
      ["end of the list", entries, "/c.txt", null],
      ["no later entry has audio", trailing, "/a.txt", null],
      ["absent current path", entries, "/missing.txt", null],
      ["null current path", entries, null, null],
    ];

    for (const [label, list, current, expected] of cases) {
      expect(nextPlayableTranscriptPath(list, current), label).toBe(expected);
    }
  });
});
