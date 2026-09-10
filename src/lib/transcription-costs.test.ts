import { describe, expect, it } from "vitest";
import {
  aggregateCosts,
  costDateError,
  defaultCostFilters,
  displayedCost,
  entryTotals,
  hourlyCost,
  localDateKey,
  parseCostDate,
  shiftCostDate,
  sonioxReferenceCost,
  formatCostTick,
  type CostFilters,
} from "./transcription-costs";
import type { TranscriptionCostEntry } from "./types";

const filters: CostFilters = {
  start: "2026-09-01",
  end: "2026-09-30",
  granularity: "day",
  provider: "all",
  model: "all",
};
function entry(
  day: string,
  overrides: Partial<TranscriptionCostEntry> = {},
): TranscriptionCostEntry {
  return {
    source_id: day,
    recorded_at: `${day}T12:00:00`,
    provider: "OpenAI",
    model_id: "openai-gpt-transcribe",
    estimated_cost_usd: 0.006,
    audio_duration_ms: 60_000,
    request_count: 1,
    unpriced_request_count: 0,
    ...overrides,
  };
}

describe("transcription cost analytics", () => {
  it("optionally estimates supported Soniox models without modifying historical records", () => {
    const realtime = entry("2026-09-01", {
      provider: "Soniox",
      model_id: "soniox-realtime-v5",
      estimated_cost_usd: null,
      audio_duration_ms: 3_600_000,
    });
    const asyncEntry = { ...realtime, model_id: "soniox-async-v5" };
    expect(sonioxReferenceCost(realtime)).toBeCloseTo(0.12);
    expect(sonioxReferenceCost(asyncEntry)).toBeCloseTo(0.1);
    const included = aggregateCosts([realtime, asyncEntry], {
      ...filters,
      includeReferenceEstimates: true,
    });
    expect(included.total.cost).toBeCloseTo(0.22);
    expect(included.total.referenceCost).toBeCloseTo(0.22);
    expect(included.total.referenceRequests).toBe(2);
    expect(included.total.unpriced).toBe(0);
    expect(
      aggregateCosts([realtime], {
        ...filters,
        includeReferenceEstimates: false,
      }).total.unpriced,
    ).toBe(1);
    expect(realtime.estimated_cost_usd).toBeNull();
  });
  it("never replaces recorded charges or invents rates for unknown models and missing duration", () => {
    const realtime = entry("2026-09-01", {
      provider: "Soniox",
      model_id: "soniox-realtime-v5",
      estimated_cost_usd: null,
    });
    expect(
      sonioxReferenceCost({ ...realtime, estimated_cost_usd: 0 }),
    ).toBeNull();
    expect(
      sonioxReferenceCost({ ...realtime, estimated_cost_usd: 0.5 }),
    ).toBeNull();
    expect(
      sonioxReferenceCost({ ...realtime, model_id: "soniox-future-v9" }),
    ).toBeNull();
    expect(
      sonioxReferenceCost({ ...realtime, audio_duration_ms: 0 }),
    ).toBeNull();
    expect(
      sonioxReferenceCost({ ...realtime, audio_duration_ms: NaN }),
    ).toBeNull();
    expect(
      sonioxReferenceCost({ ...realtime, provider: "OpenRouter" }),
    ).toBeNull();
    expect(
      entryTotals(
        { ...realtime, estimated_cost_usd: 0.5, unpriced_request_count: 1 },
        true,
      ).referenceRequests,
    ).toBe(0);
  });
  it("keeps sub-cent prices distinguishable on chart axes", () => {
    expect(formatCostTick(0.006)).toBe("$0.006");
    expect(formatCostTick(0.0002)).toBe("$0.0002");
  });
  it("includes local boundary instants and excludes invalid or out-of-range records", () => {
    const result = aggregateCosts(
      [
        entry("2026-09-01", { recorded_at: "2026-09-01T00:00:00" }),
        entry("2026-09-30", { recorded_at: "2026-09-30T23:59:59.999" }),
        entry("2026-08-31"),
        entry("2026-10-01"),
        entry("invalid"),
      ],
      filters,
    );
    expect(result.total.requests).toBe(2);
    expect(result.total.cost).toBeCloseTo(0.012);
    expect(result.buckets).toHaveLength(30);
    expect(result.buckets[1].requests).toBe(0);
    expect(result.buckets.at(-1)?.cumulative).toBeCloseTo(0.012);
  });
  it("uses Monday weeks across year boundaries and clips partial periods", () => {
    const result = aggregateCosts(
      [entry("2025-12-31"), entry("2026-01-04"), entry("2026-01-05")],
      {
        ...filters,
        start: "2025-12-31",
        end: "2026-01-06",
        granularity: "week",
      },
    );
    expect(
      result.buckets.map(({ date, end, requests }) => [date, end, requests]),
    ).toEqual([
      ["2025-12-31", "2026-01-04", 2],
      ["2026-01-05", "2026-01-06", 1],
    ]);
  });
  it("handles leap days, short months, and empty months", () => {
    const result = aggregateCosts([entry("2024-02-29"), entry("2024-04-01")], {
      ...filters,
      start: "2024-02-15",
      end: "2024-04-02",
      granularity: "month",
    });
    expect(
      result.buckets.map(({ date, end, requests }) => [date, end, requests]),
    ).toEqual([
      ["2024-02-15", "2024-02-29", 1],
      ["2024-03-01", "2024-03-31", 0],
      ["2024-04-01", "2024-04-02", 1],
    ]);
  });
  it("treats an unknown price differently from a recorded zero price", () => {
    const unknown = entryTotals(
      entry("2026-09-01", {
        estimated_cost_usd: null,
        request_count: 3,
        unpriced_request_count: 0,
      }),
    );
    expect(unknown.unpriced).toBe(3);
    expect(displayedCost(unknown)).toBe("Not priced");
    expect(hourlyCost(unknown)).toBeNull();
    const zero = entryTotals(entry("2026-09-01", { estimated_cost_usd: 0 }));
    expect(displayedCost(zero)).toBe("$0.0000");
    expect(hourlyCost(zero)).toBe(0);
  });
  it("preserves known charges in partially priced meeting aggregates without claiming an hourly rate", () => {
    const result = aggregateCosts(
      [
        entry("2026-09-01", {
          estimated_cost_usd: 0.02,
          request_count: 4,
          unpriced_request_count: 2,
        }),
      ],
      filters,
    );
    expect(result.total.cost).toBe(0.02);
    expect(result.total.unpriced).toBe(2);
    expect(hourlyCost(result.total)).toBeNull();
  });
  it("filters providers and models independently and groups identical model IDs by provider", () => {
    const entries = [
      entry("2026-09-01"),
      entry("2026-09-02", { model_id: "other" }),
      entry("2026-09-03", { provider: "Proxy" }),
    ];
    expect(aggregateCosts(entries, filters).groups).toHaveLength(3);
    expect(
      aggregateCosts(entries, { ...filters, provider: "OpenAI" }).total
        .requests,
    ).toBe(2);
    expect(
      aggregateCosts(entries, { ...filters, model: "other" }).total.requests,
    ).toBe(1);
    expect(
      aggregateCosts(entries, { ...filters, provider: "Proxy", model: "other" })
        .total.requests,
    ).toBe(0);
  });
  it("sums provider costs without turning all-unknown buckets into zero charges", () => {
    const result = aggregateCosts(
      [
        entry("2026-09-01", { provider: "Soniox", estimated_cost_usd: null }),
        entry("2026-09-01"),
        entry("2026-09-01"),
      ],
      filters,
    );
    expect(result.buckets[0].providers.Soniox).toBeNull();
    expect(result.buckets[0].providers.OpenAI).toBeCloseTo(0.012);
    expect(
      hourlyCost(result.groups.find((group) => group.provider === "OpenAI")!),
    ).toBeCloseTo(0.36);
  });
  it("handles unusual provider names without inherited object keys", () => {
    const result = aggregateCosts(
      [
        entry("2026-09-01", { provider: "__proto__" }),
        entry("2026-09-01", { provider: "constructor" }),
      ],
      filters,
    );
    expect(result.buckets[0].providers["__proto__"]).toBe(0.006);
    expect(result.buckets[0].providers.constructor).toBe(0.006);
  });
  it.each([NaN, Infinity, -3])(
    "does not present invalid cost %s as a price",
    (cost) => {
      const total = entryTotals(
        entry("2026-09-01", { estimated_cost_usd: cost }),
      );
      expect(displayedCost(total)).toBe("Not priced");
    },
  );
  it("validates impossible, reversed and excessive ranges before allocating buckets", () => {
    expect(parseCostDate("2026-02-29")).toBeNull();
    expect(costDateError("", "2026-09-01")).toMatch(/valid/);
    expect(costDateError("2026-09-02", "2026-09-01")).toMatch(/before/);
    expect(costDateError("2020-01-01", "2026-09-01")).toMatch(/two years/);
    expect(
      aggregateCosts([], { ...filters, start: "2020-01-01" }).buckets,
    ).toEqual([]);
  });
  it("uses calendar days through daylight-saving boundaries", () => {
    expect(shiftCostDate("2026-03-08", 1)).toBe("2026-03-09");
    expect(shiftCostDate("2026-11-01", 1)).toBe("2026-11-02");
    expect(defaultCostFilters(new Date(2026, 2, 10)).start).toBe("2026-02-09");
    expect(localDateKey(parseCostDate("2024-02-29")!)).toBe("2024-02-29");
  });
  it("keeps totals consistent across daily, weekly and monthly grouping", () => {
    const entries = [
      entry("2026-09-01"),
      entry("2026-09-07"),
      entry("2026-09-30", { estimated_cost_usd: null }),
    ];
    const daily = aggregateCosts(entries, filters);
    for (const granularity of ["week", "month"] as const)
      expect(
        aggregateCosts(entries, { ...filters, granularity }).total,
      ).toEqual(daily.total);
  });
});
