// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mockSnapshot } from "@/lib/app-state";
import type { RecentTranscript } from "@/lib/types";
import { RecentDictations } from "./RecentDictations";

const mocks = vi.hoisted(() => ({ loadRecentDictations: vi.fn() }));
vi.mock("@/lib/tauri-client", () => ({
  loadRecentDictations: mocks.loadRecentDictations,
}));
const entry: RecentTranscript = {
  transcript_path: "/tmp/20260906/090000-dictation.txt",
  audio_path: null,
  recorded_at: "2026-09-06T09:00:00+09:00",
  text: "Please send the revised notes.",
  source: "microphone",
  source_label: "dictation",
  device_name: "Primary microphone",
};
afterEach(cleanup);
beforeEach(() => {
  mocks.loadRecentDictations.mockReset();
});

describe("Recent Dictations", () => {
  it("expands a saved result, copies its text, and opens its day in history", async () => {
    const user = userEvent.setup();
    const writeText = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue();
    mocks.loadRecentDictations.mockResolvedValue([entry]);
    const onOpenHistory = vi.fn();
    render(
      <RecentDictations
        snapshot={mockSnapshot()}
        onOpenHistory={onOpenHistory}
      />,
    );
    await screen.findByRole("list", { name: "Recent dictations" });
    expect(mocks.loadRecentDictations).toHaveBeenCalledWith(10);
    fireEvent.click(document.querySelector("summary")!);
    expect(screen.getByText("Microphone: Primary microphone")).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Copy text" }));
    expect(writeText).toHaveBeenCalledWith(entry.text);
    expect(screen.getByRole("button", { name: "Copied" })).toBeTruthy();
    await user.click(
      screen.getByRole("button", { name: "View in Transcripts" }),
    );
    expect(onOpenHistory).toHaveBeenLastCalledWith(entry);
    await user.click(
      screen.getByRole("button", { name: "View all dictations" }),
    );
    expect(onOpenHistory).toHaveBeenLastCalledWith(entry);
  });

  it("shows an empty state and retries a failed history read", async () => {
    const user = userEvent.setup();
    mocks.loadRecentDictations
      .mockRejectedValueOnce(new Error("Disk unavailable"))
      .mockResolvedValue([]);
    render(
      <RecentDictations snapshot={mockSnapshot()} onOpenHistory={() => {}} />,
    );
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "Could not load recent dictations: Disk unavailable",
    );
    await user.click(
      screen.getByRole("button", { name: "Refresh recent dictations" }),
    );
    expect(await screen.findByText(/No recent dictations/)).toBeTruthy();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("refreshes after a completed transcript and ignores a stale storage-root response", async () => {
    let resolveOld!: (entries: RecentTranscript[]) => void;
    mocks.loadRecentDictations.mockReturnValueOnce(
      new Promise((resolve) => {
        resolveOld = resolve;
      }),
    );
    const snapshot = mockSnapshot();
    const view = render(<RecentDictations snapshot={snapshot} />);
    const next = {
      ...snapshot,
      settings: { ...snapshot.settings, save_root: "/new-root" },
    };
    mocks.loadRecentDictations.mockResolvedValue([]);
    view.rerender(<RecentDictations snapshot={next} />);
    await screen.findByText(/No recent dictations/);
    await act(async () => {
      resolveOld([entry]);
    });
    expect(screen.queryByRole("list")).toBeNull();
    mocks.loadRecentDictations.mockResolvedValue([entry]);
    view.rerender(
      <RecentDictations snapshot={{ ...next, recent_transcripts: [entry] }} />,
    );
    await waitFor(() => expect(screen.getByRole("list")).toBeTruthy());
  });

  it("reports clipboard errors without claiming a copy succeeded", async () => {
    const user = userEvent.setup();
    vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(
      new Error("Clipboard unavailable"),
    );
    mocks.loadRecentDictations.mockResolvedValue([entry]);
    render(<RecentDictations snapshot={mockSnapshot()} />);
    await screen.findByRole("list");
    fireEvent.click(document.querySelector("summary")!);
    await user.click(screen.getByRole("button", { name: "Copy text" }));
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "Could not copy dictation: Clipboard unavailable",
    );
    expect(screen.queryByRole("button", { name: "Copied" })).toBeNull();
  });
});
