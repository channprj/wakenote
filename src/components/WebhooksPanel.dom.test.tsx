// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import {
  WEBHOOK_HISTORY_WINDOW_MS,
  type WebhookHistoryEntry,
  type WebhookHistoryPage,
} from "@/lib/webhook-history";
import { WebhooksPanel } from "./WebhooksPanel";

const api = vi.hoisted(() => ({ list: vi.fn(), retry: vi.fn() }));
vi.mock("@/lib/webhook-history", async (original) => ({
  ...(await original<typeof import("@/lib/webhook-history")>()),
  listWebhookHistory: api.list,
  retryWebhookHistory: api.retry,
}));
const settings = {
  ...defaultSettings().live_transcription_webhook,
  enabled: true,
  endpoint_url: "https://example.com/hook",
};
function entry(
  id: string,
  patch: Partial<WebhookHistoryEntry> = {},
): WebhookHistoryEntry {
  return {
    event_id: id,
    created_at_ms: Date.now() - 1_000,
    updated_at_ms: Date.now(),
    status: "failed",
    endpoint_origin: "https://example.com",
    payload_format: "text_only",
    body_bytes: 42,
    attempt_count: 4,
    cycle_attempt: 4,
    manual_retries: 0,
    http_status: 503,
    error_kind: "http",
    elapsed_ms: 180,
    retry_blocked_reason: null,
    ...patch,
  };
}
function page(
  entries: WebhookHistoryEntry[],
  patch: Partial<WebhookHistoryPage> = {},
): WebhookHistoryPage {
  const counts = {
    queued: 0,
    sending: 0,
    retrying: 0,
    succeeded: 0,
    failed: 0,
    cancelled: 0,
    dropped: 0,
  };
  entries.forEach((item) => {
    counts[item.status] += 1;
  });
  return {
    entries,
    counts,
    page: 1,
    page_size: 50,
    page_count: 1,
    total: entries.length,
    now_ms: Date.now(),
    storage_warning: false,
    ...patch,
  };
}
function mount(patch: Partial<typeof settings> = {}) {
  const onOpenSettings = vi.fn();
  return {
    ...render(
      <WebhooksPanel
        settings={{ ...settings, ...patch }}
        onOpenSettings={onOpenSettings}
      />,
    ),
    onOpenSettings,
  };
}
beforeEach(() => {
  api.list.mockReset();
  api.retry.mockReset();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Webhook delivery history", () => {
  it("waits for a slow history query instead of superseding it on every refresh interval", async () => {
    vi.useFakeTimers();
    let resolve!: (result: WebhookHistoryPage) => void;
    api.list.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    mount();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(15_000);
    });
    expect(api.list).toHaveBeenCalledOnce();
    await act(async () => {
      resolve(page([entry("slow-result")]));
    });
    expect(screen.getByText("slow-result")).toBeTruthy();
  });

  it("shows results and keeps selection limited to eligible failures", async () => {
    api.list.mockResolvedValue(
      page([
        entry("failed"),
        entry("dropped", {
          status: "dropped",
          http_status: null,
          error_kind: "queue_full",
        }),
        entry("ok", {
          status: "succeeded",
          http_status: 204,
          retry_blocked_reason: "not_failed",
        }),
        entry("changed", { retry_blocked_reason: "settings_changed" }),
        entry("cancelled", {
          status: "cancelled",
          retry_blocked_reason: "not_failed",
        }),
      ]),
    );
    api.retry.mockResolvedValue({
      queued_ids: ["failed", "dropped"],
      skipped: [],
    });
    const { onOpenSettings } = mount();
    expect(await screen.findByText("HTTP 204")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: "Select failed webhooks on this page",
      }),
    );
    expect(
      (
        screen.getByRole("checkbox", {
          name: "Select webhook ok",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByText(/Restore the original endpoint/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retry selected (2)" }));
    await waitFor(() =>
      expect(api.retry).toHaveBeenCalledExactlyOnceWith(["failed", "dropped"]),
    );
    expect(await screen.findByRole("status")).toHaveProperty(
      "textContent",
      "2 requests queued for retry.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Webhook settings" }));
    expect(onOpenSettings).toHaveBeenCalledOnce();
  });

  it("paginates 50 rows at a time, resets selection, and starts filters at page one", async () => {
    const first = Array.from({ length: 50 }, (_, index) =>
      entry(`first-${index}`),
    );
    api.list.mockImplementation(async (requested, filter) =>
      filter === "needs_retry"
        ? page([entry("filtered")])
        : requested === 1
          ? page(first, { total: 51, page_count: 2 })
          : page([entry("last")], { page: 2, total: 51, page_count: 2 }),
    );
    mount();
    await screen.findByText("Page 1 of 2");
    expect(screen.getAllByRole("row")).toHaveLength(51);
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select webhook first-0" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Next Webhooks page" }));
    await screen.findByText("Page 2 of 2");
    expect(screen.queryByText("first-0")).toBeNull();
    expect(screen.getByText("0 selected")).toBeTruthy();
    expect(screen.getByText("51–51 of 51")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^Needs retry/ }));
    await screen.findByText("filtered");
    expect(api.list).toHaveBeenLastCalledWith(1, "needs_retry");
  });

  it("hides expired and future rows even with automatic deletion off", async () => {
    api.list.mockResolvedValue(
      page([
        entry("recent"),
        entry("old", { created_at_ms: Date.now() - WEBHOOK_HISTORY_WINDOW_MS }),
        entry("future", { created_at_ms: Date.now() + 60_000 }),
      ]),
    );
    mount({ auto_delete_history: false });
    await screen.findByText("recent");
    expect(screen.queryByText("old")).toBeNull();
    expect(screen.queryByText("future")).toBeNull();
  });

  it("expires a selected row while open even when subsequent refreshes fail", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-09-11T01:00:00Z"));
    api.list
      .mockResolvedValueOnce(
        page([
          entry("expiring", {
            created_at_ms: Date.now() - WEBHOOK_HISTORY_WINDOW_MS + 1_000,
          }),
        ]),
      )
      .mockRejectedValue(new Error("offline"));
    mount({ auto_delete_history: false });
    await act(async () => {});
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select webhook expiring" }),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    expect(screen.queryByText("expiring")).toBeNull();
    expect(
      (
        screen.getByRole("button", {
          name: "Retry selected",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(4_000);
    });
    expect(screen.getByRole("alert").textContent).toContain("Could not load");
    expect(api.retry).not.toHaveBeenCalled();
  });

  it("ignores a slow response from the previous filter", async () => {
    let resolve!: (result: WebhookHistoryPage) => void;
    api.list
      .mockReturnValueOnce(
        new Promise((done) => {
          resolve = done;
        }),
      )
      .mockResolvedValue(page([entry("current")]));
    mount();
    fireEvent.click(screen.getByRole("button", { name: /^Needs retry/ }));
    await screen.findByText("current");
    await act(async () => {
      resolve(page([entry("stale")]));
    });
    expect(screen.queryByText("stale")).toBeNull();
    expect(screen.getByText("current")).toBeTruthy();
  });

  it("serializes retry clicks and explains partial admission", async () => {
    let resolve!: (value: unknown) => void;
    api.list.mockResolvedValue(page([entry("one"), entry("two")]));
    api.retry.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    mount();
    await screen.findByText("one");
    fireEvent.click(
      screen.getByRole("checkbox", {
        name: "Select failed webhooks on this page",
      }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry selected (2)" }));
    fireEvent.click(screen.getByRole("button", { name: "Retrying…" }));
    expect(api.retry).toHaveBeenCalledOnce();
    await act(async () => {
      resolve({
        queued_ids: ["one"],
        skipped: [{ event_id: "two", reason: "queue_full" }],
      });
    });
    expect(screen.getByRole("status").textContent).toContain(
      "1 request queued for retry. 1 skipped.",
    );
    expect(screen.getByRole("status").textContent).toContain("queue is full");
  });

  it("blocks retries when disabled or history writes were lost", async () => {
    api.list.mockResolvedValue(page([entry("one")], { storage_warning: true }));
    mount({ enabled: false });
    await screen.findByText("one");
    expect(screen.getByText(/Webhook delivery is off/)).toBeTruthy();
    expect(screen.getByRole("alert").textContent).toContain(
      "Some history could not be saved",
    );
    expect(
      (
        screen.getByRole("checkbox", {
          name: "Select webhook one",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(api.retry).not.toHaveBeenCalled();
  });

  it("shows safe load and retry errors without private endpoint details", async () => {
    api.list
      .mockRejectedValueOnce(
        new Error("https://secret.test/private?token=secret"),
      )
      .mockResolvedValue(page([entry("one")]));
    api.retry.mockRejectedValue(new Error("/private/history.sqlite3"));
    mount();
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(document.body.textContent).not.toContain("token=secret");
    fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await screen.findByText("one");
    fireEvent.click(
      screen.getByRole("checkbox", { name: "Select webhook one" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Retry selected (1)" }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toContain(
        "Could not retry",
      ),
    );
    expect(document.body.textContent).not.toContain("/private/history.sqlite3");
  });
});
