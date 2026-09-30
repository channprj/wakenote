import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface UpdateInfo {
  status: "available" | "up_to_date" | "ahead" | "no_release";
  currentVersion: string;
  latestVersion: string | null;
  releaseUrl: string;
  notes: string;
  canInstall: boolean;
  installReason: string | null;
}

export interface UpdateProgress {
  phase: "downloading" | "verifying" | "stopping_capture" | "finishing" | "restarting";
  downloaded: number;
  total: number;
}

export interface UpdateReadiness {
  /** Work the user must finish first; installing stays disabled until then. */
  blocker: string | null;
  /** Installing pauses Capture input, which is running now. */
  stopsCapture: boolean;
  /** Capture input starts again automatically after the relaunch. */
  resumesCapture: boolean;
}

export const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000;
export const UPDATE_RETRY_INTERVAL = 60 * 60 * 1000;

export function supportsAppUpdates() {
  return typeof window !== "undefined" && isTauri();
}

export function checkForAppUpdate(): Promise<UpdateInfo> {
  return invoke("check_for_update");
}

export function updateInstallReadiness(): Promise<UpdateReadiness> {
  return invoke("update_install_readiness");
}

export function openUpdateRelease(version: string | null): Promise<void> {
  return invoke("open_update_release", { version });
}

export function installAppUpdate(expectedVersion: string): Promise<void> {
  return invoke("download_and_install_update", { expectedVersion });
}

export function onAppUpdateProgress(callback: (progress: UpdateProgress) => void) {
  return listen<UpdateProgress>("app-update-progress", (event) => callback(event.payload));
}

export function updateError(error: unknown): string {
  return error instanceof Error ? error.message : typeof error === "string" ? error : "The update could not finish. Try again.";
}
