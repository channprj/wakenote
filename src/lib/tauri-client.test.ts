import { describe, expect, it } from "vitest";
import { mockSnapshot } from "./app-state";
import {
  chooseSaveRoot,
  cancelModelDownload,
  downloadModel,
  processNextTranscription,
  startLiveCapture,
  stopLiveCapture,
} from "./tauri-client";

describe("tauri live capture client", () => {
  it("returns browser-safe snapshots for live capture actions outside Tauri", async () => {
    const expected = mockSnapshot();

    await expect(startLiveCapture()).resolves.toEqual(expected);
    await expect(stopLiveCapture()).resolves.toEqual(expected);
    await expect(processNextTranscription()).resolves.toEqual(expected);
    await expect(downloadModel("whisper-tiny")).resolves.toEqual(expected);
    await expect(cancelModelDownload("whisper-tiny")).resolves.toEqual(expected);
    await expect(chooseSaveRoot()).resolves.toEqual(expected);
  });
});
