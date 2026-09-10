import { useCallback, useEffect, useRef, useState } from "react";
import {
  ChevronLeft,
  ChevronRight,
  RefreshCw,
  RotateCw,
  Settings2,
  Webhook,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { EmptyState } from "@/components/ui/empty-state";
import { StatusBadge } from "@/components/ui/status-badge";
import type { StatusTone } from "@/lib/status-summary";
import type { LiveTranscriptionWebhookSettings } from "@/lib/types";
import {
  isRecentWebhook,
  listWebhookHistory,
  retryWebhookHistory,
  webhookReason,
  WEBHOOK_HISTORY_WINDOW_MS,
  type WebhookFilter,
  type WebhookHistoryPage,
  type WebhookStatus,
} from "@/lib/webhook-history";

const statuses: Record<WebhookStatus, { label: string; tone: StatusTone }> = {
  queued: { label: "Queued", tone: "neutral" },
  sending: { label: "Sending", tone: "primary" },
  retrying: { label: "Retrying", tone: "warning" },
  succeeded: { label: "Succeeded", tone: "success" },
  failed: { label: "Failed", tone: "danger" },
  dropped: { label: "Queue full", tone: "danger" },
  cancelled: { label: "Cancelled", tone: "neutral" },
};
const filters: { id: WebhookFilter; label: string }[] = [
  { id: "all", label: "All deliveries" },
  { id: "needs_retry", label: "Needs retry" },
  { id: "succeeded", label: "Succeeded" },
  { id: "in_progress", label: "In progress" },
  { id: "cancelled", label: "Cancelled" },
];
const emptyCounts: WebhookHistoryPage["counts"] = {
  queued: 0,
  sending: 0,
  retrying: 0,
  succeeded: 0,
  failed: 0,
  dropped: 0,
  cancelled: 0,
};

export function WebhooksPanel({
  settings,
  onOpenSettings,
}: {
  settings: LiveTranscriptionWebhookSettings;
  onOpenSettings: () => void;
}) {
  const [page, setPage] = useState(1);
  const [filter, setFilter] = useState<WebhookFilter>("all");
  const [data, setData] = useState<WebhookHistoryPage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [retrying, setRetrying] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [now, setNow] = useState(Date.now);
  const request = useRef(0);
  const loadingRef = useRef(false);
  const retryBusy = useRef(false);
  const mounted = useRef(false);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    if (retryBusy.current || loadingRef.current) return;
    loadingRef.current = true;
    const sequence = ++request.current;
    setLoading(true);
    try {
      const result = await listWebhookHistory(page, filter);
      if (sequence !== request.current || !mounted.current) return;
      setData(result);
      setNow(Date.now());
      setError(null);
      const eligible = new Set(
        result.entries
          .filter(
            (entry) =>
              isRecentWebhook(entry, Date.now()) &&
              ["failed", "dropped"].includes(entry.status) &&
              !entry.retry_blocked_reason,
          )
          .map((entry) => entry.event_id),
      );
      setSelected(
        (current) => new Set([...current].filter((id) => eligible.has(id))),
      );
    } catch {
      if (sequence === request.current && mounted.current) {
        setError(
          "Could not load webhook history. Try refreshing. Delivery continues in the background.",
        );
      }
    } finally {
      if (sequence === request.current && mounted.current) {
        loadingRef.current = false;
        setLoading(false);
      }
    }
  }, [
    page,
    filter,
    settings.enabled,
    settings.endpoint_url,
    settings.payload_format,
    settings.auto_delete_history,
  ]);

  useEffect(() => {
    setData(null);
    setSelected(new Set());
    void refresh();
    const interval = window.setInterval(() => void refresh(), 5_000);
    return () => {
      request.current += 1;
      loadingRef.current = false;
      window.clearInterval(interval);
    };
  }, [refresh]);

  useEffect(() => {
    // Expire rows even if a refresh fails or the page stays open all day.
    const interval = window.setInterval(() => setNow(Date.now()), 1_000);
    const expires =
      data?.entries.reduce((first, entry) => {
        const expiry = entry.created_at_ms + WEBHOOK_HISTORY_WINDOW_MS;
        return expiry > Date.now() ? Math.min(first, expiry) : first;
      }, Infinity) ?? Infinity;
    const timeout = Number.isFinite(expires)
      ? window.setTimeout(
          () => setNow(Date.now()),
          Math.max(0, expires - Date.now()),
        )
      : undefined;
    return () => {
      window.clearInterval(interval);
      window.clearTimeout(timeout);
    };
  }, [data]);

  const entries =
    data?.entries.filter((entry) =>
      isRecentWebhook(entry, Math.max(now, Date.now())),
    ) ?? [];
  const eligibleIds = entries
    .filter(
      (entry) =>
        settings.enabled &&
        !data?.storage_warning &&
        !entry.retry_blocked_reason &&
        ["failed", "dropped"].includes(entry.status),
    )
    .map((entry) => entry.event_id);
  const selectedIds = eligibleIds.filter((id) => selected.has(id));
  const allSelected =
    eligibleIds.length > 0 && selectedIds.length === eligibleIds.length;
  const counts = { ...(data?.counts ?? emptyCounts) };
  for (const entry of data?.entries ?? []) {
    if (!isRecentWebhook(entry, now))
      counts[entry.status] = Math.max(0, counts[entry.status] - 1);
  }
  const inProgress = counts.queued + counts.sending + counts.retrying;
  const filterCounts = {
    all: Object.values(counts).reduce((sum, count) => sum + count, 0),
    needs_retry: counts.failed + counts.dropped,
    succeeded: counts.succeeded,
    in_progress: inProgress,
    cancelled: counts.cancelled,
  };

  async function retrySelected() {
    if (retryBusy.current || selectedIds.length === 0) return;
    retryBusy.current = true;
    request.current += 1;
    loadingRef.current = false;
    setRetrying(true);
    setError(null);
    setNotice(null);
    try {
      const result = await retryWebhookHistory(selectedIds);
      if (!mounted.current) return;
      setSelected(new Set());
      const skipped = [
        ...new Set(result.skipped.map((item) => webhookReason(item.reason))),
      ].join(" ");
      setNotice(
        `${result.queued_ids.length} ${result.queued_ids.length === 1 ? "request" : "requests"} queued for retry.${result.skipped.length ? ` ${result.skipped.length} skipped. ${skipped}` : ""}`,
      );
    } catch {
      if (mounted.current)
        setError(
          "Could not retry the selected requests. Refresh history before trying again.",
        );
    } finally {
      if (mounted.current) {
        // Keep retry errors visible until the next user/periodic refresh.
        const sequence = ++request.current;
        try {
          const result = await listWebhookHistory(page, filter);
          if (sequence === request.current && mounted.current) {
            setData(result);
            setNow(Date.now());
          }
        } catch {
          if (sequence === request.current && mounted.current)
            setError(
              (current) =>
                current ??
                "Could not refresh delivery results. Refresh history to see the latest status.",
            );
        }
        if (sequence === request.current && mounted.current) {
          setLoading(false);
          setRetrying(false);
        }
      }
      retryBusy.current = false;
    }
  }

  function changePage(next: number) {
    setSelected(new Set());
    setNotice(null);
    setPage(next);
  }

  return (
    <div className="queue-panel webhooks-panel" data-slot="webhooks-panel">
      <div data-slot="queue-summary" className="queue-stats">
        {[
          { label: "In progress", count: inProgress, tone: "primary" },
          { label: "Succeeded", count: counts.succeeded, tone: "success" },
          { label: "Failed", count: counts.failed, tone: "danger" },
          { label: "Queue full", count: counts.dropped, tone: "danger" },
          { label: "Cancelled", count: counts.cancelled, tone: "neutral" },
        ].map(({ label, count, tone }) => (
          <div key={label} data-tone={count ? tone : undefined}>
            <span>{label}</span>
            <strong>{count}</strong>
          </div>
        ))}
      </div>
      <div className="toolbar">
        <Button
          variant="secondary"
          disabled={loading || retrying}
          onClick={() => void refresh()}
        >
          <RefreshCw data-icon="inline-start" />
          {loading ? "Refreshing…" : "Refresh"}
        </Button>
        <Button
          disabled={selectedIds.length === 0 || retrying || loading}
          onClick={() => void retrySelected()}
        >
          <RotateCw data-icon="inline-start" />
          {retrying
            ? "Retrying…"
            : `Retry selected${selectedIds.length ? ` (${selectedIds.length})` : ""}`}
        </Button>
        <Button variant="outline" onClick={onOpenSettings}>
          <Settings2 data-icon="inline-start" />
          Webhook settings
        </Button>
      </div>
      {!settings.enabled ? (
        <p className="webhooks-notice">
          Webhook delivery is off. Enable it in Settings to retry failed
          requests.
        </p>
      ) : null}
      {data?.storage_warning ? (
        <p role="alert" className="webhooks-notice text-destructive">
          Some history could not be saved. Manual retries are unavailable until
          WakeNote restarts; delivery continues in the background.
        </p>
      ) : null}
      {error ? (
        <p role="alert" className="webhooks-notice text-destructive">
          {error}
        </p>
      ) : null}
      {notice ? (
        <p role="status" className="webhooks-notice">
          {notice}
        </p>
      ) : null}
      <div className="queue-view-bar">
        <div className="queue-view-bar__history">
          <div
            className="queue-view-tabs"
            role="group"
            aria-label="Webhook history filters"
          >
            {filters.map((item) => (
              <Button
                key={item.id}
                size="sm"
                variant={filter === item.id ? "secondary" : "ghost"}
                aria-pressed={filter === item.id}
                disabled={retrying}
                onClick={() => {
                  setFilter(item.id);
                  changePage(1);
                }}
              >
                {item.label} <span>{filterCounts[item.id]}</span>
              </Button>
            ))}
          </div>
          <span className="queue-view-bar__hint">
            Last 24 hours · Newest first · 50 per page
          </span>
        </div>
        <span className="queue-reprocess-controls__count" aria-live="polite">
          {selectedIds.length} selected
        </span>
      </div>
      {entries.length === 0 ? (
        <EmptyState
          icon={Webhook}
          title={
            loading
              ? "Loading webhook history…"
              : error && !data
                ? "History unavailable"
                : filter === "all"
                  ? "No webhook deliveries in the last 24 hours"
                  : "No matching deliveries"
          }
          description={
            loading
              ? "Fetching recent delivery results."
              : error && !data
                ? "Refresh to try loading your history again."
                : "Newly finalized live transcripts appear here when webhook delivery is enabled."
          }
        />
      ) : (
        <div className="table-wrap webhooks-table-wrap">
          <table
            data-slot="webhooks-table"
            aria-label="Webhook delivery history"
            aria-busy={loading}
          >
            <thead>
              <tr>
                <th className="webhooks-select">
                  <Checkbox
                    aria-label="Select failed webhooks on this page"
                    checked={
                      allSelected
                        ? true
                        : selectedIds.length
                          ? "indeterminate"
                          : false
                    }
                    disabled={eligibleIds.length === 0 || loading || retrying}
                    onCheckedChange={(checked) =>
                      setSelected(
                        checked === true ? new Set(eligibleIds) : new Set(),
                      )
                    }
                  />
                </th>
                <th>Request</th>
                <th>Status</th>
                <th>Result</th>
                <th>Attempts</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => {
                const state = statuses[entry.status];
                const canRetry = eligibleIds.includes(entry.event_id);
                return (
                  <tr key={entry.event_id} data-event-id={entry.event_id}>
                    <td className="webhooks-select">
                      <Checkbox
                        aria-label={`Select webhook ${entry.event_id}`}
                        checked={canRetry && selected.has(entry.event_id)}
                        disabled={!canRetry || loading || retrying}
                        onCheckedChange={(checked) =>
                          setSelected((current) => {
                            const next = new Set(current);
                            if (checked === true) next.add(entry.event_id);
                            else next.delete(entry.event_id);
                            return next;
                          })
                        }
                      />
                    </td>
                    <td>
                      <strong
                        className="webhooks-endpoint"
                        title={entry.endpoint_origin}
                      >
                        {entry.endpoint_origin}
                      </strong>
                      <small>
                        {entry.payload_format === "json" ? "JSON" : "Text Only"}{" "}
                        · {entry.body_bytes.toLocaleString()} bytes
                      </small>
                      <code
                        className="webhooks-event-id"
                        title={entry.event_id}
                      >
                        {entry.event_id}
                      </code>
                    </td>
                    <td>
                      <StatusBadge tone={state.tone}>{state.label}</StatusBadge>
                      {["failed", "dropped"].includes(entry.status) &&
                      entry.retry_blocked_reason &&
                      settings.enabled ? (
                        <small>
                          {webhookReason(entry.retry_blocked_reason)}
                        </small>
                      ) : null}
                    </td>
                    <td>
                      <span>
                        {entry.http_status
                          ? `HTTP ${entry.http_status}`
                          : entry.error_kind
                            ? webhookReason(entry.error_kind)
                            : "—"}
                      </span>
                      {entry.elapsed_ms !== null ? (
                        <small>
                          Last attempt: {entry.elapsed_ms.toLocaleString()} ms
                        </small>
                      ) : null}
                      {entry.http_status &&
                      entry.error_kind &&
                      entry.error_kind !== "http" ? (
                        <small>{webhookReason(entry.error_kind)}</small>
                      ) : null}
                    </td>
                    <td>
                      <span>{entry.attempt_count} total</span>
                      <small>
                        {entry.manual_retries} manual{" "}
                        {entry.manual_retries === 1 ? "retry" : "retries"}
                      </small>
                    </td>
                    <td>
                      <time
                        dateTime={new Date(entry.created_at_ms).toISOString()}
                      >
                        {new Date(entry.created_at_ms).toLocaleTimeString([], {
                          hour: "2-digit",
                          minute: "2-digit",
                          second: "2-digit",
                        })}
                      </time>
                      <small>
                        {new Date(entry.created_at_ms).toLocaleDateString()}
                      </small>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {data && data.total > 0 ? (
        <nav className="queue-pagination" aria-label="Webhook history pages">
          <span className="queue-pagination__range">
            {entries.length ? (data.page - 1) * data.page_size + 1 : 0}–
            {Math.min(
              (data.page - 1) * data.page_size + entries.length,
              data.total,
            )}{" "}
            of {data.total}
          </span>
          <div className="queue-pagination__controls">
            <Button
              variant="ghost"
              size="icon"
              aria-label="Previous Webhooks page"
              disabled={loading || retrying || data.page <= 1}
              onClick={() => changePage(data.page - 1)}
            >
              <ChevronLeft data-icon="solo" />
            </Button>
            <span aria-live="polite">
              Page {data.page} of {data.page_count}
            </span>
            <Button
              variant="ghost"
              size="icon"
              aria-label="Next Webhooks page"
              disabled={loading || retrying || data.page >= data.page_count}
              onClick={() => changePage(data.page + 1)}
            >
              <ChevronRight data-icon="solo" />
            </Button>
          </div>
        </nav>
      ) : null}
    </div>
  );
}
