import { describe, expect, it, vi } from "vitest";
import { audioPlaybackUrlFromPath, initialAudioPlaybackSource } from "./audio-playback";

describe("audio playback helpers", () => {
  it("allows a Tauri audio file before converting it to an asset URL", async () => {
    const allowAudioPlayback = vi.fn().mockResolvedValue(undefined);
    const convertFileSrc = vi.fn((path: string) => `asset://localhost/${path}`);

    await expect(
      audioPlaybackUrlFromPath("/tmp/WakeNote/20260510/010203.m4a", {
        allowAudioPlayback,
        convertFileSrc,
        tauriRuntime: true,
      }),
    ).resolves.toBe("asset://localhost//tmp/WakeNote/20260510/010203.m4a");

    expect(allowAudioPlayback).toHaveBeenCalledWith("/tmp/WakeNote/20260510/010203.m4a");
    expect(convertFileSrc).toHaveBeenCalledWith("/tmp/WakeNote/20260510/010203.m4a");
  });

  it("uses a file URL in browser previews without invoking Tauri", async () => {
    const allowAudioPlayback = vi.fn();

    await expect(
      audioPlaybackUrlFromPath("/tmp/WakeNote/20260510/010203 voice.wav", {
        allowAudioPlayback,
        tauriRuntime: false,
      }),
    ).resolves.toBe("file:///tmp/WakeNote/20260510/010203%20voice.wav");

    expect(allowAudioPlayback).not.toHaveBeenCalled();
  });

  it("uses an initial source only outside Tauri", () => {
    const cases: Array<[string, boolean, string]> = [
      // No raw file URL is exposed before Tauri prepares playback.
      ["/Users/channprj/Documents/WakeNote/20260510/002816.m4a", true, ""],
      // Browser previews keep a file URL source.
      [
        "/tmp/WakeNote/20260510/010203 voice.wav",
        false,
        "file:///tmp/WakeNote/20260510/010203%20voice.wav",
      ],
    ];

    for (const [path, tauriRuntime, expected] of cases) {
      expect(
        initialAudioPlaybackSource(path, { tauriRuntime }),
        `tauriRuntime=${tauriRuntime}`,
      ).toBe(expected);
    }
  });
});
