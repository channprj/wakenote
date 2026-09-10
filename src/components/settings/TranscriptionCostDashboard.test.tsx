// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { TranscriptionCostDashboardView } from "./TranscriptionCostDashboard";

afterEach(cleanup);

describe("TranscriptionCostDashboardView", () => {
  it("opens the dedicated usage page from Details", () => {
    const onDetails = vi.fn();
    render(
      <TranscriptionCostDashboardView
        snapshot={null}
        loading={false}
        error={null}
        onRefresh={vi.fn()}
        onDetails={onDetails}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(onDetails).toHaveBeenCalledOnce();
  });
  it("shows daily weekly and monthly local API cost estimates", () => {
    render(
      <TranscriptionCostDashboardView
        loading={false}
        error={null}
        onRefresh={vi.fn()}
        snapshot={{
          currency: "USD",
          generated_at: "2026-08-03T03:00:00Z",
          today: {
            estimated_cost_usd: 0.012,
            audio_duration_ms: 120_000,
            request_count: 2,
            unpriced_request_count: 0,
          },
          week: {
            estimated_cost_usd: 0.09,
            audio_duration_ms: 900_000,
            request_count: 8,
            unpriced_request_count: 1,
          },
          month: {
            estimated_cost_usd: 1.2345,
            audio_duration_ms: 12_300_000,
            request_count: 42,
            unpriced_request_count: 3,
          },
          entry_count: 42,
          disclosure:
            "Local estimate; verify final charges in your provider billing dashboard.",
        }}
      />,
    );

    expect(screen.getByText("Today")).toBeTruthy();
    expect(screen.getByText("This week")).toBeTruthy();
    expect(screen.getByText("This month")).toBeTruthy();
    expect(screen.getByText("$0.0120")).toBeTruthy();
    expect(screen.getByText("$1.2345")).toBeTruthy();
    expect(screen.getByText(/3 unpriced/)).toBeTruthy();
    expect(screen.getByText(/verify final charges/i)).toBeTruthy();
  });
});
