import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { fileUrlFromPath } from "./transcript-history";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

interface AudioPlaybackUrlOptions {
  allowAudioPlayback?: (audioPath: string) => Promise<void>;
  convertFileSrc?: (audioPath: string) => string;
  tauriRuntime?: boolean;
}

export async function audioPlaybackUrlFromPath(
  audioPath: string,
  options: AudioPlaybackUrlOptions = {},
): Promise<string> {
  const tauriRuntime = options.tauriRuntime ?? isTauriRuntime();
  if (!tauriRuntime) {
    return fileUrlFromPath(audioPath);
  }

  const allowAudioPlayback = options.allowAudioPlayback ?? defaultAllowAudioPlayback;
  await allowAudioPlayback(audioPath);
  return (options.convertFileSrc ?? convertFileSrc)(audioPath);
}

export function initialAudioPlaybackSource(
  audioPath: string | null | undefined,
  options: Pick<AudioPlaybackUrlOptions, "tauriRuntime"> = {},
): string {
  if (!audioPath) {
    return "";
  }
  const tauriRuntime = options.tauriRuntime ?? isTauriRuntime();
  return tauriRuntime ? "" : fileUrlFromPath(audioPath);
}

function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}

async function defaultAllowAudioPlayback(audioPath: string): Promise<void> {
  await invoke("allow_audio_playback", { audioPath });
}
