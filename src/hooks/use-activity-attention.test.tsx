// @vitest-environment jsdom

import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ACTIVITY_WARNING_WINDOW_MS } from "@/lib/activity-attention";
import type { QueueJob } from "@/lib/types";
import { useActivityAttention } from "./use-activity-attention";

const occurredAt = Date.parse("2026-08-04T00:00:00.000Z");

function warningJob(id = 1, at = occurredAt): QueueJob {
  return {
    id,
    audio_path: `/tmp/${id}.wav`,
    model_id: "whisper-medium",
    status: "completed",
    issue: {
      severity: "warning",
      code: "transcript_artifact",
      message: "Transcript may contain an artifact",
      occurred_at: new Date(at).toISOString(),
    },
  };
}

describe("useActivityAttention", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("expires a warning at exactly ten minutes with one timeout and no interval", () => {
    vi.setSystemTime(occurredAt + ACTIVITY_WARNING_WINDOW_MS - 1);
    const intervalSpy = vi.spyOn(window, "setInterval");
    const { result, unmount } = renderHook(() =>
      useActivityAttention([warningJob()]),
    );

    expect(result.current.attention?.tone).toBe("warning");
    expect(vi.getTimerCount()).toBe(1);
    expect(intervalSpy).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1));
    expect(result.current.attention).toBeNull();
    expect(vi.getTimerCount()).toBe(0);

    unmount();
    intervalSpy.mockRestore();
  });

  it("clears the old timeout when a newer warning changes the boundary", () => {
    vi.setSystemTime(occurredAt + 1_000);
    const { result, rerender, unmount } = renderHook(
      ({ jobs }) => useActivityAttention(jobs),
      { initialProps: { jobs: [warningJob()] } },
    );
    expect(vi.getTimerCount()).toBe(1);

    const newerAt = occurredAt + 900;
    rerender({ jobs: [warningJob(2, newerAt)] });
    expect(result.current.attention?.nextExpiryAt).toBe(
      newerAt + ACTIVITY_WARNING_WINDOW_MS,
    );
    expect(vi.getTimerCount()).toBe(1);

    unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
