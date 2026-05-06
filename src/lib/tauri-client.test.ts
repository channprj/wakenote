import { describe, expect, it } from "vitest";
import { mockSnapshot } from "./app-state";
import { startLiveCapture, stopLiveCapture } from "./tauri-client";

describe("tauri live capture client", () => {
  it("returns browser-safe snapshots for live capture actions outside Tauri", async () => {
    const expected = mockSnapshot();

    await expect(startLiveCapture()).resolves.toEqual(expected);
    await expect(stopLiveCapture()).resolves.toEqual(expected);
  });
});
