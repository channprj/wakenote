// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RecentTranscript } from "@/lib/types";
import { TranscriptsView } from "./TranscriptsView";

afterEach(cleanup);

const day = "2026-05-10";
const entry: RecentTranscript = {
  transcript_path: "/tmp/WakeNote/20260510/010203.txt",
  audio_path: "/tmp/WakeNote/20260510/010203.m4a",
  recorded_at: "2026-05-10T01:02:03+09:00",
  text: "Transcript selected for visibility",
};

function renderVisibilityView(
  onSetTranscriptsHidden: (
    entries: readonly RecentTranscript[],
    hidden: boolean,
  ) => boolean | Promise<boolean>,
) {
  render(
    <TranscriptsView
      today={new Date("2026-05-10T12:00:00+09:00")}
      days={[{ day, count: 1 }]}
      entriesByDay={new Map([[day, [entry]]])}
      visibleCountByDay={new Map([[day, 1]])}
      hiddenCountByDay={new Map([[day, 0]])}
      onSetTranscriptsHidden={onSetTranscriptsHidden}
    />,
  );
}

describe("TranscriptsView list visibility", () => {
  it("sends the selected transcript paths as one hide batch", async () => {
    const onSetTranscriptsHidden = vi.fn().mockResolvedValue(true);
    renderVisibilityView(onSetTranscriptsHidden);

    await userEvent.click(
      screen.getByRole("checkbox", {
        name: "Select transcript 2026-05-10 01:02:03",
      }),
    );
    const apply = screen.getByRole("button", {
      name: "Hide selected",
    }) as HTMLButtonElement;
    await userEvent.click(apply);

    expect(onSetTranscriptsHidden).toHaveBeenCalledWith(
      [entry],
      true,
    );
    expect(apply.disabled).toBe(true);
  });

  it("keeps selection intact when persistence rejects the batch", async () => {
    const onSetTranscriptsHidden = vi.fn().mockResolvedValue(false);
    renderVisibilityView(onSetTranscriptsHidden);

    const checkbox = screen.getByRole("checkbox", {
      name: "Select transcript 2026-05-10 01:02:03",
    });
    await userEvent.click(checkbox);
    await userEvent.click(
      screen.getByRole("button", { name: "Hide selected" }),
    );

    expect(checkbox.getAttribute("aria-checked")).toBe("true");
  });

  it("offers visibility beside regeneration in the context menu", async () => {
    const onSetTranscriptsHidden = vi.fn().mockResolvedValue(true);
    renderVisibilityView(onSetTranscriptsHidden);
    const row = screen
      .getByText("Transcript selected for visibility")
      .closest(".transcript-entry");
    expect(row).not.toBeNull();

    fireEvent.contextMenu(row as Element, {
      clientX: 20,
      clientY: 20,
    });
    await userEvent.click(
      screen.getByRole("menuitem", { name: "Hide selected" }),
    );

    expect(onSetTranscriptsHidden).toHaveBeenCalledWith(
      [entry],
      true,
    );
  });
});
