// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  loadTranscriptionCostDetails,
  subscribeTranscriptionCostUpdates,
} from "@/lib/tauri-client";
import { useTranscriptionCostDetails } from "./use-transcription-cost-details";
import type { TranscriptionCostDetails } from "@/lib/types";

vi.mock("@/lib/tauri-client", () => ({
  loadTranscriptionCostDetails: vi.fn(),
  subscribeTranscriptionCostUpdates: vi.fn(),
}));
const data = (generated_at: string): TranscriptionCostDetails => ({
  currency: "USD",
  generated_at,
  entries: [],
  entry_limit: 10_000,
});
const deferred = <T,>() => {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
};
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(subscribeTranscriptionCostUpdates).mockResolvedValue(vi.fn());
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
describe("useTranscriptionCostDetails", () => {
  it("ignores old responses that finish after a newer refresh", async () => {
    const first = deferred<TranscriptionCostDetails>();
    vi.mocked(loadTranscriptionCostDetails)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce(data("new"));
    const { result } = renderHook(useTranscriptionCostDetails);
    await act(async () => {
      await result.current.refresh();
    });
    await act(async () => first.resolve(data("old")));
    expect(result.current.details?.generated_at).toBe("new");
  });
  it("coalesces update events and refreshes the detailed ledger", async () => {
    vi.useFakeTimers();
    vi.mocked(loadTranscriptionCostDetails).mockResolvedValue(data("loaded"));
    renderHook(useTranscriptionCostDetails);
    await act(async () => {});
    const callback = vi.mocked(subscribeTranscriptionCostUpdates).mock
      .calls[0][0];
    act(() => {
      callback({} as never);
      callback({} as never);
      callback({} as never);
    });
    await act(() => vi.advanceTimersByTimeAsync(500));
    expect(loadTranscriptionCostDetails).toHaveBeenCalledTimes(2);
  });
  it("retains the last ledger on failure and hides raw error details", async () => {
    vi.mocked(loadTranscriptionCostDetails)
      .mockResolvedValueOnce(data("saved"))
      .mockRejectedValueOnce(new Error("private/path/token"));
    const { result } = renderHook(useTranscriptionCostDetails);
    await waitFor(() =>
      expect(result.current.details?.generated_at).toBe("saved"),
    );
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.details?.generated_at).toBe("saved");
    expect(result.current.error).toBe(
      "Could not load local API usage. Try refreshing.",
    );
    expect(result.current.loading).toBe(false);
  });
  it("cleans up subscriptions that resolve after unmount", async () => {
    const subscription = deferred<() => void>();
    const stop = vi.fn();
    vi.mocked(subscribeTranscriptionCostUpdates).mockReturnValue(
      subscription.promise,
    );
    vi.mocked(loadTranscriptionCostDetails).mockResolvedValue(data("loaded"));
    const { unmount } = renderHook(useTranscriptionCostDetails);
    unmount();
    await act(async () => subscription.resolve(stop));
    expect(stop).toHaveBeenCalledOnce();
  });
  it("handles a failed subscription without an unhandled rejection", async () => {
    vi.mocked(loadTranscriptionCostDetails).mockResolvedValue(data("loaded"));
    vi.mocked(subscribeTranscriptionCostUpdates).mockRejectedValue(
      new Error("offline"),
    );
    const { result } = renderHook(useTranscriptionCostDetails);
    await waitFor(() =>
      expect(result.current.error).toContain(
        "Automatic updates are unavailable",
      ),
    );
    expect(result.current.details?.generated_at).toBe("loaded");
  });
});
