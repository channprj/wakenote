// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { LlmReportRunSnapshot } from "@/lib/types";

const mocks = vi.hoisted(() => ({
  list: vi.fn(),
  subscribe: vi.fn(),
}));

vi.mock("@/lib/tauri-client", () => ({
  listLlmReportRuns: mocks.list,
  subscribeLlmReportRuns: mocks.subscribe,
}));

import {
  llmReportRunsReducer,
  useLlmReportRuns,
} from "./use-llm-report-runs";

function run(
  overrides: Partial<LlmReportRunSnapshot> = {},
): LlmReportRunSnapshot {
  return {
    run_id: "run-1",
    parent_run_id: null,
    revision: 1,
    status: "queued",
    stage: null,
    kind: "summary",
    created_at: "2026-07-18T00:00:00Z",
    updated_at: "2026-07-18T00:00:00Z",
    started_at: null,
    finished_at: null,
    iteration: 0,
    max_iterations: 3,
    message: "Queued for report generation",
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
}

describe("llmReportRunsReducer", () => {
  it("does not let stale hydration replace a newer event revision", () => {
    const eventState = llmReportRunsReducer(
      { runs: [], loading: true, error: null },
      {
        type: "update",
        run: run({
          revision: 3,
          status: "running",
          stage: "generating",
        }),
      },
    );

    const hydrated = llmReportRunsReducer(eventState, {
      type: "hydrate",
      runs: [run({ revision: 1 })],
    });

    expect(hydrated.runs[0]).toMatchObject({
      revision: 3,
      status: "running",
      stage: "generating",
    });
  });
});

describe("useLlmReportRuns", () => {
  beforeEach(() => {
    mocks.list.mockReset();
    mocks.subscribe.mockReset();
  });

  it("subscribes before hydrating and keeps events newer than the list response", async () => {
    let onRun: ((run: LlmReportRunSnapshot) => void) | undefined;
    let resolveList:
      | ((runs: LlmReportRunSnapshot[]) => void)
      | undefined;
    const unlisten = vi.fn();
    mocks.subscribe.mockImplementation(
      async (listener: (run: LlmReportRunSnapshot) => void) => {
        onRun = listener;
        return unlisten;
      },
    );
    mocks.list.mockImplementation(
      () =>
        new Promise<LlmReportRunSnapshot[]>((resolve) => {
          resolveList = resolve;
        }),
    );

    const { result, unmount } = renderHook(() => useLlmReportRuns());

    await waitFor(() => expect(mocks.list).toHaveBeenCalledOnce());
    expect(
      mocks.subscribe.mock.invocationCallOrder[0],
    ).toBeLessThan(mocks.list.mock.invocationCallOrder[0]);

    act(() => {
      onRun?.(
        run({
          revision: 3,
          status: "running",
          stage: "generating",
        }),
      );
    });
    await act(async () => {
      resolveList?.([run({ revision: 1 })]);
    });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.runs[0]).toMatchObject({
      revision: 3,
      status: "running",
    });
    expect(result.current.activeRun?.run_id).toBe("run-1");

    unmount();
    expect(unlisten).toHaveBeenCalledOnce();
  });

  it("disposes a subscription that resolves after unmount", async () => {
    let resolveSubscription:
      | ((unlisten: () => void) => void)
      | undefined;
    const unlisten = vi.fn();
    mocks.subscribe.mockImplementation(
      () =>
        new Promise<() => void>((resolve) => {
          resolveSubscription = resolve;
        }),
    );
    mocks.list.mockResolvedValue([]);

    const { unmount } = renderHook(() => useLlmReportRuns());
    unmount();
    await act(async () => {
      resolveSubscription?.(unlisten);
    });

    expect(unlisten).toHaveBeenCalledOnce();
    expect(mocks.list).not.toHaveBeenCalled();
  });
});
