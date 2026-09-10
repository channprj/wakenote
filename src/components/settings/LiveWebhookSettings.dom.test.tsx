// @vitest-environment jsdom
import { StrictMode } from "react";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import { LiveWebhookSettings } from "./LiveWebhookSettings";

const initial = () => defaultSettings().live_transcription_webhook;
const changeUrl = (value: string) =>
  fireEvent.change(screen.getByLabelText("Endpoint URL"), {
    target: { value },
  });
const tick = (ms: number) => act(() => vi.advanceTimersByTimeAsync(ms));
const toggle = () =>
  fireEvent.click(screen.getByRole("switch", { name: "Enable Webhook" }));
beforeEach(() => vi.useFakeTimers());
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Live Transcription Webhook autosave", () => {
  it("does not write on mount, StrictMode replay, or persisted prop refresh", async () => {
    const onSave = vi.fn();
    const { rerender, unmount } = render(
      <StrictMode>
        <LiveWebhookSettings settings={initial()} onSave={onSave} />
      </StrictMode>,
    );
    expect(
      screen.queryByRole("button", { name: /save webhook|cancel changes/i }),
    ).toBeNull();
    expect(screen.getByText("Text Only")).toBeTruthy();
    rerender(
      <StrictMode>
        <LiveWebhookSettings
          settings={{
            ...initial(),
            endpoint_url: "https://example.com/stored",
          }}
          onSave={onSave}
        />
      </StrictMode>,
    );
    await tick(5000);
    expect(
      (screen.getByLabelText("Endpoint URL") as HTMLInputElement).value,
    ).toBe("https://example.com/stored");
    unmount();
    expect(onSave).not.toHaveBeenCalled();
  });

  it("automatically persists switches without enabling delivery", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    fireEvent.click(
      screen.getByRole("switch", {
        name: "Automatically delete old webhook history",
      }),
    );
    await tick(0);
    expect(onSave).toHaveBeenCalledExactlyOnceWith({
      live_transcription_webhook: { ...initial(), auto_delete_history: false },
    });
    expect(screen.getByRole("status").textContent).toBe(
      "Settings saved automatically.",
    );
  });

  it("waits for 700 ms of quiet typing and saves the trimmed latest URL atomically", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    changeUrl("https://example.com/a");
    await tick(600);
    changeUrl(" https://example.com/final ");
    toggle();
    await tick(699);
    expect(onSave).not.toHaveBeenCalled();
    await tick(1);
    expect(onSave).toHaveBeenCalledExactlyOnceWith({
      live_transcription_webhook: {
        ...initial(),
        enabled: true,
        endpoint_url: "https://example.com/final",
      },
    });
  });

  it("throttles successive writes to one per second and coalesces switch changes", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <LiveWebhookSettings
        settings={{ ...initial(), endpoint_url: "https://example.com" }}
        onSave={onSave}
      />,
    );
    toggle();
    await tick(0);
    toggle();
    fireEvent.click(
      screen.getByRole("switch", {
        name: "Automatically delete old webhook history",
      }),
    );
    await tick(999);
    expect(onSave).toHaveBeenCalledTimes(1);
    await tick(1);
    expect(onSave).toHaveBeenLastCalledWith({
      live_transcription_webhook: {
        ...initial(),
        endpoint_url: "https://example.com",
        auto_delete_history: false,
      },
    });
  });

  it("does not save during continuous typing or IME composition", async () => {
    const onSave = vi.fn();
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    for (const suffix of ["a", "ab", "abc"]) {
      changeUrl(`https://example.com/${suffix}`);
      await tick(600);
    }
    expect(onSave).not.toHaveBeenCalled();
    fireEvent.compositionStart(screen.getByLabelText("Endpoint URL"));
    changeUrl("https://example.com/한글");
    await tick(3000);
    expect(onSave).not.toHaveBeenCalled();
    fireEvent.compositionEnd(screen.getByLabelText("Endpoint URL"));
    await tick(699);
    expect(onSave).not.toHaveBeenCalled();
    await tick(1);
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("preserves newer edits during slow writes and honors their debounce", async () => {
    let resolve!: () => void;
    const onSave = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<void>((done) => {
            resolve = done;
          }),
      )
      .mockResolvedValue(undefined);
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    changeUrl("https://example.com/first");
    await tick(700);
    await tick(2000);
    changeUrl("https://example.com/latest");
    expect(
      (screen.getByLabelText("Endpoint URL") as HTMLInputElement).disabled,
    ).toBe(false);
    await act(async () => resolve());
    await tick(699);
    expect(onSave).toHaveBeenCalledTimes(1);
    await tick(1);
    expect(onSave).toHaveBeenLastCalledWith({
      live_transcription_webhook: {
        ...initial(),
        endpoint_url: "https://example.com/latest",
      },
    });
  });

  it("rejects invalid enabled URLs but allows disabling with an invalid old URL", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <LiveWebhookSettings
        settings={{
          ...initial(),
          enabled: true,
          endpoint_url: "https://example.com",
        }}
        onSave={onSave}
      />,
    );
    changeUrl("file:///private/test");
    await tick(700);
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toMatch(/valid http/);
    expect(
      screen.getByLabelText("Endpoint URL").getAttribute("aria-invalid"),
    ).toBe("true");
    toggle();
    await tick(0);
    expect(onSave).toHaveBeenCalledExactlyOnceWith({
      live_transcription_webhook: {
        ...initial(),
        endpoint_url: "file:///private/test",
      },
    });
  });

  it("preserves drafts through polling and uses the latest save callback without resetting debounce", async () => {
    const onSave = vi.fn();
    const newerSave = vi.fn();
    const { rerender } = render(
      <LiveWebhookSettings settings={initial()} onSave={onSave} />,
    );
    changeUrl("https://example.com/draft");
    await tick(600);
    rerender(
      <LiveWebhookSettings
        settings={{
          ...initial(),
          endpoint_url: "https://example.com/elsewhere",
        }}
        onSave={newerSave}
      />,
    );
    await tick(100);
    expect(onSave).not.toHaveBeenCalled();
    expect(newerSave).toHaveBeenCalledExactlyOnceWith({
      live_transcription_webhook: {
        ...initial(),
        endpoint_url: "https://example.com/draft",
      },
    });
  });

  it("retains failed changes, hides secrets, and waits for an explicit retry", async () => {
    const onSave = vi
      .fn()
      .mockRejectedValueOnce(new Error("https://private.test/token=secret"))
      .mockResolvedValue(undefined);
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    changeUrl("https://example.com/draft");
    await tick(700);
    expect(screen.getByRole("alert").textContent).toMatch(
      /changes are preserved/,
    );
    expect(document.body.textContent).not.toContain("token=secret");
    await tick(10000);
    expect(onSave).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Retry saving" }));
    await tick(0);
    expect(onSave).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("saves the latest draft after an older write fails", async () => {
    let reject!: (cause: Error) => void;
    const onSave = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<void>((_, fail) => {
            reject = fail;
          }),
      )
      .mockResolvedValue(undefined);
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    changeUrl("https://example.com/first");
    await tick(700);
    changeUrl("https://example.com/latest");
    await act(async () => reject(new Error("failed")));
    await tick(1000);
    expect(onSave).toHaveBeenCalledTimes(2);
    expect(onSave).toHaveBeenLastCalledWith({
      live_transcription_webhook: {
        ...initial(),
        endpoint_url: "https://example.com/latest",
      },
    });
  });

  it("does not write reverted or whitespace-only changes", async () => {
    const onSave = vi.fn();
    render(
      <LiveWebhookSettings
        settings={{ ...initial(), endpoint_url: "https://example.com" }}
        onSave={onSave}
      />,
    );
    changeUrl("https://example.com/other");
    changeUrl("https://example.com");
    await tick(1000);
    changeUrl(" https://example.com ");
    await tick(1000);
    expect(onSave).not.toHaveBeenCalled();
  });

  it("flushes valid pending edits once when navigating away", async () => {
    const onSave = vi.fn();
    const { unmount } = render(
      <LiveWebhookSettings settings={initial()} onSave={onSave} />,
    );
    changeUrl("https://example.com/final");
    unmount();
    await tick(5000);
    expect(onSave).toHaveBeenCalledExactlyOnceWith({
      live_transcription_webhook: {
        ...initial(),
        endpoint_url: "https://example.com/final",
      },
    });
  });

  it("does not duplicate in-flight saves when unmounted", async () => {
    let resolve!: () => void;
    const onSave = vi.fn(
      () =>
        new Promise<void>((done) => {
          resolve = done;
        }),
    );
    const { unmount } = render(
      <LiveWebhookSettings settings={initial()} onSave={onSave} />,
    );
    changeUrl("https://example.com/final");
    await tick(700);
    unmount();
    await act(async () => resolve());
    await tick(5000);
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("does not flush invalid or known-failed changes on navigation", async () => {
    const onSave = vi.fn().mockRejectedValue(new Error("failed"));
    const first = render(
      <LiveWebhookSettings
        settings={{ ...initial(), enabled: true }}
        onSave={onSave}
      />,
    );
    changeUrl("invalid");
    first.unmount();
    expect(onSave).not.toHaveBeenCalled();
    const second = render(
      <LiveWebhookSettings settings={initial()} onSave={onSave} />,
    );
    changeUrl("https://example.com/failed");
    await tick(700);
    second.unmount();
    expect(onSave).toHaveBeenCalledTimes(1);
  });
});
