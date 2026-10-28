import { describe, expect, it } from "vitest";
import {
  REPORT_KINDS,
  canGenerateReport,
  defaultSelectedDays,
  describeReportScope,
  reportComposerBlockedReason,
  reportKindLabel,
  selectedCaptureCount,
  selectedDateRange,
  sortDaysDescending,
  toggleDaySelection,
} from "./report-composer";
import type { TranscriptDay } from "./types";

const days: TranscriptDay[] = [
  { day: "2026-07-23", count: 2 },
  { day: "2026-07-27", count: 4 },
  { day: "2026-07-25", count: 3 },
];

function gate(overrides: Partial<Parameters<typeof canGenerateReport>[0]> = {}) {
  return {
    openrouterKeyConfigured: true,
    hasActiveRun: false,
    selectedCaptureCount: 4,
    submitting: false,
    ...overrides,
  };
}

describe("report kinds", () => {
  it("describes exactly the two backend report kinds", () => {
    expect(REPORT_KINDS.map((entry) => entry.kind)).toEqual([
      "summary",
      "detailed_report",
    ]);
    expect(reportKindLabel("summary")).toBe("Summary");
    expect(reportKindLabel("detailed_report")).toBe("Detailed report");

    // A detailed report warns that it costs more.
    const detailed = REPORT_KINDS.find(
      (entry) => entry.kind === "detailed_report",
    );
    expect(detailed?.description).toMatch(/costs more/);
  });
});

describe("sortDaysDescending", () => {
  it("puts the newest day first without mutating the input", () => {
    const sorted = sortDaysDescending(days);

    expect(sorted.map((day) => day.day)).toEqual([
      "2026-07-27",
      "2026-07-25",
      "2026-07-23",
    ]);
    expect(days[0].day).toBe("2026-07-23");
  });
});

describe("selectedCaptureCount", () => {
  it("sums capture counts of selected days only", () => {
    const cases: Array<[string[], number]> = [
      [["2026-07-27", "2026-07-23"], 6],
      // Nothing selected.
      [[], 0],
      // Selected days that have no captures are ignored.
      [["2099-01-01"], 0],
    ];

    for (const [selection, expected] of cases) {
      expect(selectedCaptureCount(days, new Set(selection)), selection.join(",")).toBe(expected);
    }
  });
});

describe("selectedDateRange", () => {
  it("spans the selected days or returns null", () => {
    const cases: Array<[string[], ReturnType<typeof selectedDateRange>]> = [
      // Earliest and latest selected day.
      [["2026-07-27", "2026-07-23", "2026-07-25"], { start: "2026-07-23", end: "2026-07-27" }],
      // A single day collapses into an equal start and end.
      [["2026-07-27"], { start: "2026-07-27", end: "2026-07-27" }],
      // No selection.
      [[], null],
    ];

    for (const [selection, expected] of cases) {
      expect(selectedDateRange(new Set(selection)), selection.join(",")).toEqual(expected);
    }
  });
});

describe("describeReportScope", () => {
  it("describes the report scope for each selection shape", () => {
    const cases: Array<[string, TranscriptDay[], string[], string]> = [
      ["single day", days, ["2026-07-27"], "4 captures from 2026-07-27"],
      ["singular for one capture", [{ day: "2026-07-27", count: 1 }], ["2026-07-27"], "1 capture from 2026-07-27"],
      ["multi-day span", days, ["2026-07-23", "2026-07-27"], "6 captures across 2 days · 2026-07-23 → 2026-07-27"],
      ["nothing selected", days, [], "No captures selected"],
    ];

    for (const [label, scopeDays, selection, expected] of cases) {
      expect(describeReportScope(scopeDays, new Set(selection)), label).toBe(expected);
    }
  });
});

describe("reportComposerBlockedReason", () => {
  it("explains the first blocker for each generation gate", () => {
    const cases: Array<{
      label: string;
      overrides: Parameters<typeof gate>[0];
      reason: RegExp | null;
      canGenerate?: boolean;
    }> = [
      {
        label: "key, free runner, and captures present",
        overrides: {},
        reason: null,
        canGenerate: true,
      },
      {
        // A missing OpenRouter key is reported before anything else.
        label: "every blocker at once",
        overrides: {
          openrouterKeyConfigured: false,
          hasActiveRun: true,
          selectedCaptureCount: 0,
        },
        reason: /OpenRouter API key/,
      },
      { label: "another run active", overrides: { hasActiveRun: true }, reason: /already being generated/ },
      { label: "empty selection", overrides: { selectedCaptureCount: 0 }, reason: /at least one day/ },
      {
        label: "start in flight",
        overrides: { submitting: true },
        reason: /Starting the report/,
        canGenerate: false,
      },
    ];

    for (const { label, overrides, reason, canGenerate } of cases) {
      if (reason === null) {
        expect(reportComposerBlockedReason(gate(overrides)), label).toBeNull();
      } else {
        expect(reportComposerBlockedReason(gate(overrides)), label).toMatch(reason);
      }
      if (canGenerate !== undefined) {
        expect(canGenerateReport(gate(overrides)), label).toBe(canGenerate);
      }
    }
  });
});

describe("toggleDaySelection", () => {
  it("toggles a day into a new set without mutating the input", () => {
    const added = toggleDaySelection(new Set(), "2026-07-27");
    expect([...added]).toEqual(["2026-07-27"]);

    expect([...toggleDaySelection(added, "2026-07-27")]).toEqual([]);

    const original = new Set(["2026-07-27"]);
    toggleDaySelection(original, "2026-07-25");
    expect([...original]).toEqual(["2026-07-27"]);
  });
});

describe("defaultSelectedDays", () => {
  it("preselects the newest day when any exist", () => {
    // The dialog opens ready to generate.
    expect([...defaultSelectedDays(days)]).toEqual(["2026-07-27"]);
    expect([...defaultSelectedDays([])]).toEqual([]);
  });
});
