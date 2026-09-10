// @vitest-environment jsdom
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import { LiveWebhookSettings } from "./LiveWebhookSettings";

afterEach(cleanup);

const initial = () => defaultSettings().live_transcription_webhook;

function changeUrl(value: string) {
  fireEvent.change(screen.getByLabelText("Endpoint URL"), {
    target: { value },
  });
}

describe("Live Transcription Webhook settings", () => {
  it("defaults automatic history deletion on and persists an opt out without enabling delivery", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    const toggle = screen.getByRole("switch", {
      name: "Automatically delete old webhook history",
    });
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(toggle);
    fireEvent.click(
      screen.getByRole("button", { name: "Save webhook settings" }),
    );
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledExactlyOnceWith({
        live_transcription_webhook: {
          ...initial(),
          auto_delete_history: false,
        },
      }),
    );
    expect(
      screen.getByText(/older history stays on this Mac but remains hidden/),
    ).toBeTruthy();
  });

  it("starts disabled with Text Only and does not save or send on mount", () => {
    const onSave = vi.fn();
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    expect(
      screen
        .getByRole("switch", { name: "Enable Webhook" })
        .getAttribute("aria-checked"),
    ).toBe("false");
    expect(
      (screen.getByLabelText("Endpoint URL") as HTMLInputElement).value,
    ).toBe("");
    expect(screen.getByText("Text Only")).toBeTruthy();
    expect(onSave).not.toHaveBeenCalled();
    expect(
      screen.getByText(/15-second timeout and up to 3 retries/),
    ).toBeTruthy();
  });

  it("blocks invalid enabled URLs next to the input and saves the entire setting atomically", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(<LiveWebhookSettings settings={initial()} onSave={onSave} />);
    fireEvent.click(screen.getByRole("switch", { name: "Enable Webhook" }));
    changeUrl("file:///private/transcripts");
    fireEvent.click(
      screen.getByRole("button", { name: "Save webhook settings" }),
    );
    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByRole("alert").textContent).toMatch(/valid http/);
    expect(
      screen.getByLabelText("Endpoint URL").getAttribute("aria-invalid"),
    ).toBe("true");
    changeUrl(" https://example.com/hook?token=test ");
    fireEvent.click(
      screen.getByRole("button", { name: "Save webhook settings" }),
    );
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledExactlyOnceWith({
        live_transcription_webhook: {
          enabled: true,
          endpoint_url: "https://example.com/hook?token=test",
          payload_format: "text_only",
          auto_delete_history: true,
        },
      }),
    );
  });

  it("allows HTTP with a privacy notice and describes JSON metadata", async () => {
    const user = userEvent.setup();
    render(
      <LiveWebhookSettings
        settings={{ ...initial(), payload_format: "json" }}
        onSave={vi.fn()}
      />,
    );
    changeUrl("http://127.0.0.1:8765/hook");
    expect(
      screen.getByText(/HTTP sends transcripts without encryption/),
    ).toBeTruthy();
    expect(
      screen.getByText(/JSON includes the transcript and metadata/),
    ).toBeTruthy();
    expect(
      screen.getByText(/data already sent cannot be recalled/),
    ).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Cancel changes" }));
    expect(
      (screen.getByLabelText("Endpoint URL") as HTMLInputElement).value,
    ).toBe("");
  });

  it("allows disabling even if the old endpoint is invalid", async () => {
    const onSave = vi.fn().mockResolvedValue(undefined);
    render(
      <LiveWebhookSettings
        settings={{ ...initial(), enabled: true, endpoint_url: "broken" }}
        onSave={onSave}
      />,
    );
    fireEvent.click(screen.getByRole("switch", { name: "Enable Webhook" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Save webhook settings" }),
    );
    await waitFor(() =>
      expect(onSave).toHaveBeenCalledWith({
        live_transcription_webhook: { ...initial(), endpoint_url: "broken" },
      }),
    );
  });

  it("preserves drafts and hides endpoint secrets when persistence fails", async () => {
    const onSave = vi
      .fn()
      .mockRejectedValueOnce(new Error("https://private.test/token=secret"))
      .mockResolvedValue(undefined);
    const { rerender } = render(
      <LiveWebhookSettings settings={initial()} onSave={onSave} />,
    );
    changeUrl("https://example.com/new");
    fireEvent.click(
      screen.getByRole("button", { name: "Save webhook settings" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(
        /changes are preserved/,
      ),
    );
    expect(document.body.textContent).not.toContain("token=secret");
    rerender(
      <LiveWebhookSettings
        settings={{
          ...initial(),
          endpoint_url: "https://changed-elsewhere.test",
        }}
        onSave={onSave}
      />,
    );
    expect(
      (screen.getByLabelText("Endpoint URL") as HTMLInputElement).value,
    ).toBe("https://example.com/new");
    fireEvent.click(
      screen.getByRole("button", { name: "Save webhook settings" }),
    );
    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(2));
  });

  it("serializes saves and restores persisted settings on a fresh mount", async () => {
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
    changeUrl("https://example.com/hook");
    fireEvent.click(screen.getByRole("switch", { name: "Enable Webhook" }));
    fireEvent.click(
      screen.getByRole("button", { name: "Save webhook settings" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Saving…" }));
    expect(onSave).toHaveBeenCalledOnce();
    resolve();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Save webhook settings" }),
      ).toBeTruthy(),
    );
    unmount();
    render(
      <LiveWebhookSettings
        settings={{
          enabled: true,
          endpoint_url: "https://example.com/hook",
          payload_format: "json",
          auto_delete_history: true,
        }}
        onSave={onSave}
      />,
    );
    expect(
      screen
        .getByRole("switch", { name: "Enable Webhook" })
        .getAttribute("aria-checked"),
    ).toBe("true");
    expect(screen.getByText("JSON")).toBeTruthy();
    expect(
      (screen.getByLabelText("Endpoint URL") as HTMLInputElement).value,
    ).toBe("https://example.com/hook");
    expect(onSave).toHaveBeenCalledOnce();
  });
});
