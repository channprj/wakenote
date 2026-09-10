import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime, loadSnapshot } from "./tauri-client";
import type { LiveTranscriptionWebhookSettings } from "./types";

export const WEBHOOK_HISTORY_WINDOW_MS = 86_400_000;
export type WebhookStatus =
  | "queued"
  | "sending"
  | "retrying"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "dropped";
export type WebhookFilter =
  | "all"
  | "needs_retry"
  | "succeeded"
  | "in_progress"
  | "cancelled";
export interface WebhookHistoryEntry {
  event_id: string;
  created_at_ms: number;
  updated_at_ms: number;
  status: WebhookStatus;
  endpoint_origin: string;
  payload_format: LiveTranscriptionWebhookSettings["payload_format"];
  body_bytes: number;
  attempt_count: number;
  cycle_attempt: number;
  manual_retries: number;
  http_status: number | null;
  error_kind: string | null;
  elapsed_ms: number | null;
  retry_blocked_reason: string | null;
}
export interface WebhookHistoryPage {
  entries: WebhookHistoryEntry[];
  counts: Record<WebhookStatus, number>;
  page: number;
  page_size: number;
  page_count: number;
  total: number;
  now_ms: number;
  storage_warning: boolean;
}
export interface WebhookRetryResult {
  queued_ids: string[];
  skipped: { event_id: string; reason: string }[];
}

export function isRecentWebhook(entry: WebhookHistoryEntry, now: number) {
  return (
    entry.created_at_ms > now - WEBHOOK_HISTORY_WINDOW_MS &&
    entry.created_at_ms <= now
  );
}

export function webhookReason(reason: string): string {
  return (
    (
      {
        disabled: "Enable the webhook in Settings to retry.",
        settings_changed:
          "Restore the original endpoint and payload format to retry.",
        not_failed: "This request is no longer failed.",
        expired: "This request is more than 24 hours old.",
        not_found: "This request is no longer available.",
        queue_full: "The delivery queue is full. Try again shortly.",
        shutting_down: "WakeNote is shutting down.",
        timeout: "Timed out",
        network: "Network error",
        http: "HTTP error",
        interrupted: "Interrupted when WakeNote closed",
      } as Record<string, string>
    )[reason] ?? "Request could not be delivered."
  );
}

// Browser development fixtures never make webhook requests. The desktop app
// always uses its persisted history through the commands below.
let browserEntries: WebhookHistoryEntry[] = [];
const fixtureEndpoint = "https://example.com/hook";

export function seedBrowserWebhookHistory() {
  const now = Date.now();
  const statuses: WebhookStatus[] = [
    "succeeded",
    "failed",
    "succeeded",
    "dropped",
    "cancelled",
  ];
  browserEntries = Array.from({ length: 76 }, (_, index) => {
    const status = statuses[index % statuses.length];
    const created =
      now -
      (index === 75 ? WEBHOOK_HISTORY_WINDOW_MS + 1 : (index + 1) * 60_000);
    return {
      event_id: `preview-webhook-${String(index + 1).padStart(3, "0")}`,
      created_at_ms: created,
      updated_at_ms: created + 500,
      status,
      endpoint_origin: "https://example.com",
      payload_format: "text_only",
      body_bytes: 120 + index * 17,
      attempt_count: status === "failed" ? 4 : status === "dropped" ? 0 : 1,
      cycle_attempt: status === "failed" ? 4 : status === "dropped" ? 0 : 1,
      manual_retries: 0,
      http_status:
        status === "succeeded" ? 200 : status === "failed" ? 503 : null,
      error_kind:
        status === "failed"
          ? "http"
          : status === "dropped"
            ? "queue_full"
            : null,
      elapsed_ms: status === "dropped" ? null : 140 + index * 3,
      retry_blocked_reason: null,
    };
  });
}

export async function listWebhookHistory(
  page: number,
  filter: WebhookFilter,
): Promise<WebhookHistoryPage> {
  if (isTauriRuntime()) return invoke("list_webhook_history", { page, filter });
  const settings = (await loadSnapshot()).settings.live_transcription_webhook;
  const now = Date.now();
  if (settings.auto_delete_history)
    browserEntries = browserEntries.filter((entry) =>
      isRecentWebhook(entry, now),
    );
  const recent = browserEntries.filter((entry) => isRecentWebhook(entry, now));
  const counts: WebhookHistoryPage["counts"] = {
    queued: 0,
    sending: 0,
    retrying: 0,
    succeeded: 0,
    failed: 0,
    cancelled: 0,
    dropped: 0,
  };
  for (const entry of recent) counts[entry.status] += 1;
  const filtered = recent.filter(
    (entry) =>
      filter === "all" ||
      (filter === "needs_retry"
        ? ["failed", "dropped"].includes(entry.status)
        : filter === "in_progress"
          ? ["queued", "sending", "retrying"].includes(entry.status)
          : entry.status === filter),
  );
  const pageCount = Math.max(1, Math.ceil(filtered.length / 50));
  const currentPage = Math.min(Math.max(page, 1), pageCount);
  return {
    entries: filtered
      .slice((currentPage - 1) * 50, currentPage * 50)
      .map((entry) => ({
        ...entry,
        retry_blocked_reason: !["failed", "dropped"].includes(entry.status)
          ? "not_failed"
          : !settings.enabled
            ? "disabled"
            : settings.endpoint_url !== fixtureEndpoint ||
                settings.payload_format !== entry.payload_format
              ? "settings_changed"
              : null,
      })),
    counts,
    page: currentPage,
    page_size: 50,
    page_count: pageCount,
    total: filtered.length,
    now_ms: now,
    storage_warning: false,
  };
}

export async function retryWebhookHistory(
  eventIds: string[],
): Promise<WebhookRetryResult> {
  if (isTauriRuntime()) return invoke("retry_webhook_history", { eventIds });
  const settings = (await loadSnapshot()).settings.live_transcription_webhook;
  const result: WebhookRetryResult = { queued_ids: [], skipped: [] };
  for (const id of new Set(eventIds)) {
    const entry = browserEntries.find((item) => item.event_id === id);
    const reason = !entry
      ? "not_found"
      : !isRecentWebhook(entry, Date.now())
        ? "expired"
        : !["failed", "dropped"].includes(entry.status)
          ? "not_failed"
          : !settings.enabled
            ? "disabled"
            : settings.endpoint_url !== fixtureEndpoint ||
                settings.payload_format !== entry.payload_format
              ? "settings_changed"
              : null;
    if (reason || !entry) {
      result.skipped.push({ event_id: id, reason: reason ?? "not_found" });
    } else {
      entry.status = "succeeded";
      entry.manual_retries += 1;
      entry.attempt_count += 1;
      entry.cycle_attempt = 1;
      entry.http_status = 200;
      entry.error_kind = null;
      entry.updated_at_ms = Date.now();
      result.queued_ids.push(id);
    }
  }
  return result;
}
