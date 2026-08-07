// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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

    expect(onSetTranscriptsHidden).toHaveBeenCalledWith([entry], true);
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

    expect(onSetTranscriptsHidden).toHaveBeenCalledWith([entry], true);
  });
});

describe("TranscriptsView audio merge selection", () => {
  it("offers both selected audio files without clearing selection", async () => {
    const second: RecentTranscript = {
      ...entry,
      transcript_path: "/tmp/WakeNote/20260510/010204.txt",
      audio_path: "/tmp/WakeNote/20260510/010204.wav",
      recorded_at: "2026-05-10T01:02:04+09:00",
      text: "Second transcript",
    };
    const onMergeAudio = vi.fn().mockResolvedValue(undefined);
    render(
      <TranscriptsView
        today={new Date("2026-05-10T12:00:00+09:00")}
        days={[{ day, count: 2 }]}
        entriesByDay={new Map([[day, [entry, second]]])}
        onMergeAudio={onMergeAudio}
      />,
    );

    const checkboxes = screen.getAllByRole("checkbox", {
      name: /Select transcript/,
    });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    const merge = screen.getByRole("button", { name: "Merge Audio · 2" });
    await userEvent.click(merge);

    expect(onMergeAudio).toHaveBeenCalledWith([entry, second]);
    expect(checkboxes[0].getAttribute("aria-checked")).toBe("true");
    expect(checkboxes[1].getAttribute("aria-checked")).toBe("true");
  });
});

describe("TranscriptsView recoverable deletion", () => {
  it("confirms and moves the exact selected Transcript bundle to Trash", async () => {
    const onTrashTranscripts = vi
      .fn()
      .mockResolvedValue([entry.transcript_path]);
    render(
      <TranscriptsView
        today={new Date("2026-05-10T12:00:00+09:00")}
        days={[{ day, count: 1 }]}
        entriesByDay={new Map([[day, [entry]]])}
        onTrashTranscripts={onTrashTranscripts}
      />,
    );

    const checkbox = screen.getByRole("checkbox", {
      name: "Select transcript 2026-05-10 01:02:03",
    });
    await userEvent.click(checkbox);
    await userEvent.click(
      screen.getByRole("button", {
        name: "Move selected transcript bundles to Trash",
      }),
    );
    expect(screen.getByText("Move 1 transcript to Trash?")).toBeTruthy();
    expect(
      screen.getByText(/audio, transcript text, metadata JSON, and error text/),
    ).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Move bundles to Trash" }),
    );

    expect(onTrashTranscripts).toHaveBeenCalledWith([entry]);
    expect(checkbox.getAttribute("aria-checked")).toBe("false");
  });

  it("preserves failed Transcript selections for retry", async () => {
    const onTrashTranscripts = vi.fn().mockResolvedValue([]);
    render(
      <TranscriptsView
        today={new Date("2026-05-10T12:00:00+09:00")}
        days={[{ day, count: 1 }]}
        entriesByDay={new Map([[day, [entry]]])}
        onTrashTranscripts={onTrashTranscripts}
      />,
    );

    const checkbox = screen.getByRole("checkbox", {
      name: "Select transcript 2026-05-10 01:02:03",
    });
    await userEvent.click(checkbox);
    await userEvent.click(
      screen.getByRole("button", {
        name: "Move selected transcript bundles to Trash",
      }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Move bundles to Trash" }),
    );

    expect(checkbox.getAttribute("aria-checked")).toBe("true");
  });
});
