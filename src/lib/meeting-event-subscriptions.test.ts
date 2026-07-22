import { describe, expect, it, vi } from "vitest";

import { subscribeMeetingEvents } from "./meeting-event-subscriptions";

describe("subscribeMeetingEvents", () => {
  it("returns every cleanup handle before the caller hydrates meetings", async () => {
    const order: string[] = [];
    const listen = vi.fn(async (eventName: string) => {
      order.push(`listen:${eventName}`);
      return vi.fn();
    });

    const unlisteners = await subscribeMeetingEvents(listen, {
      onProgress: vi.fn(),
      onSegment: vi.fn(),
      onFinished: vi.fn(),
    });
    order.push("hydrate");

    expect(order).toEqual([
      "listen:meeting-progress",
      "listen:meeting-segment-committed",
      "listen:meeting-finished",
      "hydrate",
    ]);
    expect(unlisteners).toHaveLength(3);
  });

  it("cleans up earlier listeners when later registration fails", async () => {
    const unlistenProgress = vi.fn();
    const listen = vi
      .fn()
      .mockResolvedValueOnce(unlistenProgress)
      .mockRejectedValueOnce(new Error("segment listener failed"));

    await expect(
      subscribeMeetingEvents(listen, {
        onProgress: vi.fn(),
        onSegment: vi.fn(),
        onFinished: vi.fn(),
      }),
    ).rejects.toThrow("segment listener failed");

    expect(unlistenProgress).toHaveBeenCalledOnce();
  });
});
