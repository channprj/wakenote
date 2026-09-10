// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TranscriptionCostsView } from "./TranscriptionCostsPage";
import {
  aggregateCosts,
  defaultCostFilters,
  shiftCostDate,
} from "@/lib/transcription-costs";
import type { TranscriptionCostDetails } from "@/lib/types";

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
function details(): TranscriptionCostDetails {
  const end = defaultCostFilters().end;
  return {
    currency: "USD",
    generated_at: new Date().toISOString(),
    entry_limit: 10_000,
    entries: [
      {
        source_id: "meeting:known",
        recorded_at: `${end}T12:00:00`,
        provider: "OpenAI",
        model_id: "openai-test",
        estimated_cost_usd: 0.06,
        audio_duration_ms: 600_000,
        request_count: 3,
        unpriced_request_count: 0,
      },
      {
        source_id: "dictation:unknown",
        recorded_at: `${shiftCostDate(end, -6)}T12:00:00`,
        provider: "Soniox",
        model_id: "soniox-test",
        estimated_cost_usd: null,
        audio_duration_ms: 300_000,
        request_count: 2,
        unpriced_request_count: 2,
      },
    ],
  };
}
function view(data: TranscriptionCostDetails | null = details(), extra = {}) {
  const onBack = vi.fn();
  const onRefresh = vi.fn();
  render(
    <TranscriptionCostsView
      details={data}
      loading={false}
      error={null}
      onBack={onBack}
      onRefresh={onRefresh}
      {...extra}
    />,
  );
  return { onBack, onRefresh };
}

describe("transcription cost details page", () => {
  it("discloses incomplete history when a capped ledger overlaps the requested dates", () => {
    view({ ...details(), entry_limit: 2 });
    expect(
      screen.getByText("Earlier usage may no longer be retained"),
    ).toBeTruthy();
    expect(screen.getByText(/these totals may be incomplete/)).toBeTruthy();
  });
  it("lets users include and exclude Soniox reference estimates with clear attribution", () => {
    const data = details();
    data.entries[1].model_id = "soniox-realtime-v5";
    view(data);
    const totals = within(screen.getByLabelText("Selected period totals"));
    expect(totals.getByText("$0.0700")).toBeTruthy();
    expect(
      screen.getByText("Includes duration-based reference estimates"),
    ).toBeTruthy();
    fireEvent.click(
      screen.getByRole("switch", {
        name: "Include Soniox reference estimates",
      }),
    );
    expect(totals.getByText("$0.0600")).toBeTruthy();
    expect(
      screen.queryByText("Includes duration-based reference estimates"),
    ).toBeNull();
    expect(totals.getByText("60%")).toBeTruthy();
  });
  it("shows real usage, partial pricing, both chart types and model comparisons", () => {
    view();
    expect(Element.prototype.scrollIntoView).toHaveBeenCalledWith({
      block: "start",
    });
    expect(
      screen.getByRole("group", {
        name: "Estimated cost by provider and period",
      }),
    ).toBeTruthy();
    expect(
      screen.getByRole("group", { name: "Cumulative estimated cost trend" }),
    ).toBeTruthy();
    const totals = within(screen.getByLabelText("Selected period totals"));
    expect(totals.getByText("$0.0600")).toBeTruthy();
    expect(totals.getByText("15 min")).toBeTruthy();
    expect(totals.getByText("60%")).toBeTruthy();
    expect(
      screen.getByLabelText("Estimated cost by provider and period"),
    ).toBeTruthy();
    expect(
      screen.getByLabelText("Cumulative estimated cost trend"),
    ).toBeTruthy();
    expect(screen.getByText("$0.3600")).toBeTruthy();
    for (const name of [
      "API and model cost comparison",
      "Cost period breakdown",
    ]) {
      const table = screen.getByRole("table", { name });
      table.focus();
      expect(document.activeElement).toBe(table);
    }
    expect(screen.getAllByText("Not priced").length).toBeGreaterThan(0);
    expect(
      screen.getByText(/missing prices do not mean free usage/),
    ).toBeTruthy();
  });
  it("changes daily, weekly and monthly grouping without changing totals", () => {
    const data = details();
    view(data);
    for (const [label, granularity] of [
      ["Weekly", "week"],
      ["Monthly", "month"],
      ["Daily", "day"],
    ] as const) {
      fireEvent.click(screen.getByRole("radio", { name: label }));
      expect(screen.getByRole("status").textContent).toContain(
        `${aggregateCosts(data.entries, { ...defaultCostFilters(), granularity }).buckets.length} periods`,
      );
      expect(
        within(screen.getByLabelText("Selected period totals")).getByText(
          "$0.0600",
        ),
      ).toBeTruthy();
    }
  });
  it("switches the line chart from cumulative charges to audio usage", () => {
    view();
    fireEvent.click(screen.getByRole("radio", { name: "Audio time" }));
    expect(screen.getByLabelText("Audio minutes trend")).toBeTruthy();
    expect(
      screen.queryByLabelText("Cumulative estimated cost trend"),
    ).toBeNull();
  });
  it("filters by API and clears an incompatible model when the API changes", () => {
    view();
    fireEvent.keyDown(screen.getByLabelText("API provider"), {
      key: "ArrowDown",
    });
    fireEvent.click(screen.getByRole("option", { name: "Soniox" }));
    expect(
      within(screen.getByLabelText("Selected period totals")).getByText(
        "Not priced",
      ),
    ).toBeTruthy();
    expect(screen.queryByText("openai-test")).toBeNull();
    expect(screen.getByText("soniox-test")).toBeTruthy();
  });
  it("paginates exact daily values and resets to page one after a range change", () => {
    view();
    fireEvent.click(screen.getByRole("button", { name: "Next" }));
    expect(screen.getByRole("status").textContent).toMatch(/Page 2/);
    fireEvent.click(screen.getByRole("button", { name: "Last 7 days" }));
    expect(screen.getByRole("status").textContent).toBe(
      "Page 1 of 1 · 7 periods",
    );
  });
  it("validates reversed and impossible dates without rendering misleading zero totals", () => {
    view();
    fireEvent.change(screen.getByLabelText("Start date"), {
      target: { value: shiftCostDate(defaultCostFilters().end, 1) },
    });
    expect(screen.getByRole("alert").textContent).toMatch(/Start date must/);
    expect(screen.queryByLabelText("Selected period totals")).toBeNull();
    fireEvent.change(screen.getByLabelText("Start date"), {
      target: { value: "" },
    });
    expect(screen.getByRole("alert").textContent).toMatch(/valid start/);
  });
  it("shows a useful empty state, refresh and back navigation", () => {
    const { onBack, onRefresh } = view({ ...details(), entries: [] });
    expect(screen.getByText("No API usage in this range")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Back to Models" }));
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(onBack).toHaveBeenCalledOnce();
    expect(onRefresh).toHaveBeenCalledOnce();
  });
  it("shows initial loading and preserves old data alongside a refresh error", () => {
    view(null, { loading: true });
    expect(screen.getByText("Loading API usage…")).toBeTruthy();
    expect(screen.queryByLabelText("Selected period totals")).toBeNull();
    cleanup();
    view(details(), {
      error: "Could not load local API usage. Try refreshing.",
    });
    expect(screen.getByText(/Showing the last loaded records/)).toBeTruthy();
    expect(
      within(screen.getByLabelText("Selected period totals")).getByText(
        "$0.0600",
      ),
    ).toBeTruthy();
  });
});
