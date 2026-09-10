import type { TranscriptionCostEntry } from "./types";

export type CostGranularity = "day" | "week" | "month";
export interface CostFilters {
  start: string;
  end: string;
  granularity: CostGranularity;
  provider: string;
  model: string;
  includeReferenceEstimates?: boolean;
}
export interface CostTotals {
  cost: number;
  duration: number;
  requests: number;
  unpriced: number;
  pricedEntries: number;
  referenceRequests: number;
  referenceCost: number;
}
export interface CostGroup extends CostTotals {
  provider: string;
  model: string;
}
export interface CostBucket extends CostTotals {
  date: string;
  end: string;
  cumulative: number;
  providers: Record<string, number | null>;
}

export function localDateKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

// Calendar arithmetic uses local dates, not fixed 24-hour milliseconds. This
// keeps midnight, DST transitions, leap days and Monday weeks consistent.
export function parseCostDate(value: string): Date | null {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return null;
  const [year, month, day] = value.split("-").map(Number);
  const date = new Date(year, month - 1, day);
  return localDateKey(date) === value ? date : null;
}

export function shiftCostDate(value: string, days: number): string {
  const date = parseCostDate(value)!;
  date.setDate(date.getDate() + days);
  return localDateKey(date);
}

export function costDateError(start: string, end: string): string | null {
  if (!parseCostDate(start) || !parseCostDate(end))
    return "Choose a valid start and end date.";
  if (start > end) return "Start date must be on or before end date.";
  if (end > shiftCostDate(start, 731))
    return "Choose a range of up to two years (732 days).";
  return null;
}

export function defaultCostFilters(now = new Date()): CostFilters {
  const end = localDateKey(now);
  return {
    start: shiftCostDate(end, -29),
    end,
    granularity: "day",
    provider: "all",
    model: "all",
    includeReferenceEstimates: true,
  };
}

const empty = (): CostTotals => ({
  cost: 0,
  duration: 0,
  requests: 0,
  unpriced: 0,
  pricedEntries: 0,
  referenceRequests: 0,
  referenceCost: 0,
});
const nonnegative = (value: number) =>
  Number.isFinite(value) ? Math.max(0, value) : 0;
// Soniox publishes approximate audio-hour equivalents for token-based billing.
// These are comparison estimates, never recorded charges or ledger mutations.
// Source: https://soniox.com/pricing, checked 2026-09-11 (Asia/Seoul).
export const SONIOX_REFERENCE_RATES = { async: 0.1, realtime: 0.12 } as const;
export function sonioxReferenceCost(
  entry: TranscriptionCostEntry,
): number | null {
  if (
    entry.provider !== "Soniox" ||
    entry.estimated_cost_usd !== null ||
    !Number.isFinite(entry.audio_duration_ms) ||
    entry.audio_duration_ms <= 0
  )
    return null;
  const rate =
    entry.model_id === "soniox-async-v5"
      ? SONIOX_REFERENCE_RATES.async
      : entry.model_id === "soniox-realtime-v5"
        ? SONIOX_REFERENCE_RATES.realtime
        : null;
  return rate === null ? null : (entry.audio_duration_ms / 3_600_000) * rate;
}

export function entryTotals(
  entry: TranscriptionCostEntry,
  includeReferenceEstimates = false,
): CostTotals {
  const requests = Math.floor(nonnegative(entry.request_count));
  const reference = includeReferenceEstimates
    ? sonioxReferenceCost(entry)
    : null;
  const cost = reference ?? entry.estimated_cost_usd;
  const priced = cost !== null && Number.isFinite(cost) && cost >= 0;
  return {
    cost: priced ? cost! : 0,
    duration: nonnegative(entry.audio_duration_ms),
    requests,
    unpriced:
      reference !== null
        ? 0
        : priced
          ? Math.min(
              requests,
              Math.floor(nonnegative(entry.unpriced_request_count)),
            )
          : requests,
    pricedEntries: Number(priced),
    referenceRequests: reference !== null ? requests : 0,
    referenceCost: reference ?? 0,
  };
}

function add(total: CostTotals, entry: CostTotals) {
  total.cost += entry.cost;
  total.duration += entry.duration;
  total.requests += entry.requests;
  total.unpriced += entry.unpriced;
  total.pricedEntries += entry.pricedEntries;
  total.referenceRequests += entry.referenceRequests;
  total.referenceCost += entry.referenceCost;
}

function bucketDate(date: Date, granularity: CostGranularity): string {
  const next = new Date(date);
  if (granularity === "week")
    next.setDate(next.getDate() - ((next.getDay() + 6) % 7));
  if (granularity === "month") next.setDate(1);
  return localDateKey(next);
}

export function aggregateCosts(
  entries: TranscriptionCostEntry[],
  filters: CostFilters,
) {
  const total = empty();
  const groups = new Map<string, CostGroup>();
  const buckets = new Map<string, CostBucket>();
  const selected: TranscriptionCostEntry[] = [];
  if (costDateError(filters.start, filters.end))
    return { total, groups: [], buckets: [], entries: [] };
  const start = parseCostDate(filters.start)!;
  const endExclusive = parseCostDate(shiftCostDate(filters.end, 1))!;
  for (
    const cursor = new Date(start);
    cursor < endExclusive;
    cursor.setDate(cursor.getDate() + 1)
  ) {
    const key = bucketDate(cursor, filters.granularity);
    const existing = buckets.get(key);
    if (existing) existing.end = localDateKey(cursor);
    else
      buckets.set(key, {
        ...empty(),
        date: localDateKey(cursor),
        end: localDateKey(cursor),
        cumulative: 0,
        providers: Object.create(null) as Record<string, number | null>,
      });
  }
  for (const entry of entries) {
    const date = new Date(entry.recorded_at);
    if (
      !Number.isFinite(date.getTime()) ||
      date < start ||
      date >= endExclusive
    )
      continue;
    if (filters.provider !== "all" && entry.provider !== filters.provider)
      continue;
    if (filters.model !== "all" && entry.model_id !== filters.model) continue;
    const value = entryTotals(entry, filters.includeReferenceEstimates);
    selected.push(entry);
    add(total, value);
    const key = JSON.stringify([entry.provider, entry.model_id]);
    const group = groups.get(key) ?? {
      ...empty(),
      provider: entry.provider,
      model: entry.model_id,
    };
    add(group, value);
    groups.set(key, group);
    const bucket = buckets.get(bucketDate(date, filters.granularity))!;
    add(bucket, value);
    // Unknown charges remain null. An API used without a known rate must not
    // look like a free provider in the chart or comparison table.
    const previous = bucket.providers[entry.provider];
    bucket.providers[entry.provider] = value.pricedEntries
      ? (previous ?? 0) + value.cost
      : (previous ?? null);
  }
  let cumulative = 0;
  for (const bucket of buckets.values()) {
    cumulative += bucket.cost;
    bucket.cumulative = cumulative;
  }
  return {
    total,
    groups: [...groups.values()].sort(
      (a, b) =>
        b.cost - a.cost ||
        b.duration - a.duration ||
        a.provider.localeCompare(b.provider) ||
        a.model.localeCompare(b.model),
    ),
    buckets: [...buckets.values()],
    entries: selected.sort((a, b) =>
      b.recorded_at.localeCompare(a.recorded_at),
    ),
  };
}

export function formatCost(value: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 4,
    maximumFractionDigits: 4,
  }).format(value);
}

export function formatCostTick(value: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumSignificantDigits: 3,
  }).format(value);
}
export function displayedCost(total: CostTotals): string {
  return total.requests > 0 && total.pricedEntries === 0
    ? "Not priced"
    : formatCost(total.cost);
}
export function hourlyCost(total: CostTotals): number | null {
  return total.unpriced === 0 && total.pricedEntries > 0 && total.duration > 0
    ? total.cost / (total.duration / 3_600_000)
    : null;
}
export function formatCostDuration(milliseconds: number): string {
  return `${new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(milliseconds / 60_000)} min`;
}
