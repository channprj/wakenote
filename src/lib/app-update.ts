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
  phase: "downloading" | "verifying" | "restarting";
  downloaded: number;
  total: number;
}

export const UPDATE_CHECK_INTERVAL = 24 * 60 * 60 * 1000;
export const UPDATE_RETRY_INTERVAL = 60 * 60 * 1000;

export function supportsAppUpdates() {
  return typeof window !== "undefined" && isTauri();
}

export function checkForAppUpdate(): Promise<UpdateInfo> {
  return invoke("check_for_update");
}

export function updateInstallBlocker(): Promise<string | null> {
  return invoke("update_install_blocker");
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
