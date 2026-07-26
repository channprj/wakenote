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
  it("offers exactly the two kinds the backend supports", () => {
    expect(REPORT_KINDS.map((entry) => entry.kind)).toEqual([
      "summary",
      "detailed_report",
    ]);
  });

  it("labels each kind for display", () => {
    expect(reportKindLabel("summary")).toBe("Summary");
    expect(reportKindLabel("detailed_report")).toBe("Detailed report");
  });

  it("warns that a detailed report costs more", () => {
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
  it("sums the counts of selected days only", () => {
    expect(
      selectedCaptureCount(days, new Set(["2026-07-27", "2026-07-23"])),
    ).toBe(6);
  });

  it("is zero when nothing is selected", () => {
    expect(selectedCaptureCount(days, new Set())).toBe(0);
  });

  it("ignores selected days that have no captures", () => {
    expect(selectedCaptureCount(days, new Set(["2099-01-01"]))).toBe(0);
  });
});

describe("selectedDateRange", () => {
  it("spans the earliest and latest selected day", () => {
    expect(
      selectedDateRange(new Set(["2026-07-27", "2026-07-23", "2026-07-25"])),
    ).toEqual({ start: "2026-07-23", end: "2026-07-27" });
  });

  it("collapses a single day into an equal start and end", () => {
    expect(selectedDateRange(new Set(["2026-07-27"]))).toEqual({
      start: "2026-07-27",
      end: "2026-07-27",
    });
  });

  it("is null with no selection", () => {
    expect(selectedDateRange(new Set())).toBeNull();
  });
});

describe("describeReportScope", () => {
  it("states what a single-day report will cover", () => {
    expect(describeReportScope(days, new Set(["2026-07-27"]))).toBe(
      "4 captures from 2026-07-27",
    );
  });

  it("uses the singular for one capture", () => {
    expect(
      describeReportScope([{ day: "2026-07-27", count: 1 }], new Set(["2026-07-27"])),
    ).toBe("1 capture from 2026-07-27");
  });

  it("states the span for a multi-day report", () => {
    expect(
      describeReportScope(days, new Set(["2026-07-23", "2026-07-27"])),
    ).toBe("6 captures across 2 days · 2026-07-23 → 2026-07-27");
  });

  it("says so when nothing is selected", () => {
    expect(describeReportScope(days, new Set())).toBe("No captures selected");
  });
});

describe("reportComposerBlockedReason", () => {
  it("allows generation once a key, a free runner, and captures are present", () => {
    expect(reportComposerBlockedReason(gate())).toBeNull();
    expect(canGenerateReport(gate())).toBe(true);
  });

  it("blocks on a missing OpenRouter key before anything else", () => {
    const reason = reportComposerBlockedReason(
      gate({
        openrouterKeyConfigured: false,
        hasActiveRun: true,
        selectedCaptureCount: 0,
      }),
    );

    expect(reason).toMatch(/OpenRouter API key/);
  });

  it("blocks while another run is active", () => {
    expect(
      reportComposerBlockedReason(gate({ hasActiveRun: true })),
    ).toMatch(/already being generated/);
  });

  it("blocks on an empty selection", () => {
    expect(
      reportComposerBlockedReason(gate({ selectedCaptureCount: 0 })),
    ).toMatch(/at least one day/);
  });

  it("blocks re-submitting while a start is in flight", () => {
    expect(reportComposerBlockedReason(gate({ submitting: true }))).toMatch(
      /Starting the report/,
    );
    expect(canGenerateReport(gate({ submitting: true }))).toBe(false);
  });
});

describe("toggleDaySelection", () => {
  it("adds an unselected day and removes a selected one", () => {
    const added = toggleDaySelection(new Set(), "2026-07-27");
    expect([...added]).toEqual(["2026-07-27"]);

    expect([...toggleDaySelection(added, "2026-07-27")]).toEqual([]);
  });

  it("does not mutate the incoming set", () => {
    const original = new Set(["2026-07-27"]);
    toggleDaySelection(original, "2026-07-25");

    expect([...original]).toEqual(["2026-07-27"]);
  });
});

describe("defaultSelectedDays", () => {
  it("preselects the newest day so the dialog opens ready to generate", () => {
    expect([...defaultSelectedDays(days)]).toEqual(["2026-07-27"]);
  });

  it("selects nothing when there are no days", () => {
    expect([...defaultSelectedDays([])]).toEqual([]);
  });
});
