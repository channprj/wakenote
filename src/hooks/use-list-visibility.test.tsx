// @vitest-environment jsdom

import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { emptyListVisibilityState } from "@/lib/list-visibility";
import type { ListVisibilityState } from "@/lib/types";

const mocks = vi.hoisted(() => ({
  load: vi.fn(),
  set: vi.fn(),
}));

vi.mock("@/lib/tauri-client", () => ({
  loadListVisibility: mocks.load,
  setListVisibility: mocks.set,
}));

import { useListVisibility } from "./use-list-visibility";

function hiddenMeetingState(): ListVisibilityState {
  return {
    ...emptyListVisibilityState(),
    meetings: ["meeting-1"],
  };
}

describe("useListVisibility", () => {
  beforeEach(() => {
    mocks.load.mockReset();
    mocks.set.mockReset();
  });

  it("loads fail-open state and commits the server-returned mutation", async () => {
    mocks.load.mockResolvedValue(emptyListVisibilityState());
    mocks.set.mockResolvedValue(hiddenMeetingState());
    const { result } = renderHook(() => useListVisibility());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.state).toEqual(emptyListVisibilityState());

    await act(async () => {
      expect(
        await result.current.setTargetsHidden(
          [{ kind: "meeting", id: "meeting-1" }],
          true,
        ),
      ).toBe(true);
    });

    expect(mocks.set).toHaveBeenCalledWith({
      targets: [{ kind: "meeting", id: "meeting-1" }],
      hidden: true,
    });
    expect(result.current.state).toEqual(hiddenMeetingState());
    expect(result.current.announcement).toBe(
      "Hidden from list · Files remain on disk",
    );
  });

  it("keeps committed state intact when a mutation fails", async () => {
    mocks.load.mockResolvedValue(hiddenMeetingState());
    mocks.set.mockRejectedValue(new Error("registry is read-only"));
    const { result } = renderHook(() => useListVisibility());

    await waitFor(() => expect(result.current.loading).toBe(false));
    await act(async () => {
      expect(
        await result.current.setTargetsHidden(
          [{ kind: "meeting", id: "meeting-1" }],
          false,
        ),
      ).toBe(false);
    });

    expect(result.current.state).toEqual(hiddenMeetingState());
    expect(result.current.error).toContain("registry is read-only");
  });

  it("rejects repeated submission while one whole batch is pending", async () => {
    let resolveMutation: ((state: ListVisibilityState) => void) | undefined;
    mocks.load.mockResolvedValue(emptyListVisibilityState());
    mocks.set.mockImplementation(
      () =>
        new Promise<ListVisibilityState>((resolve) => {
          resolveMutation = resolve;
        }),
    );
    const { result } = renderHook(() => useListVisibility());

    await waitFor(() => expect(result.current.loading).toBe(false));
    let first!: Promise<boolean>;
    act(() => {
      first = result.current.setTargetsHidden(
        [{ kind: "meeting", id: "meeting-1" }],
        true,
      );
    });
    await waitFor(() => expect(result.current.mutating).toBe(true));

    await act(async () => {
      expect(
        await result.current.setTargetsHidden(
          [{ kind: "meeting", id: "meeting-2" }],
          true,
        ),
      ).toBe(false);
      resolveMutation?.(hiddenMeetingState());
      expect(await first).toBe(true);
    });

    expect(mocks.set).toHaveBeenCalledOnce();
  });

  it("surfaces a corrupt-load error while leaving all items visible", async () => {
    mocks.load.mockRejectedValue(new Error("json error: expected value"));
    const { result } = renderHook(() => useListVisibility());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.state).toEqual(emptyListVisibilityState());
    expect(result.current.error).toContain("json error");
  });
});
