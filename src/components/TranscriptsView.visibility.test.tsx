// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RecentTranscript } from "@/lib/types";
import { TranscriptsView } from "./TranscriptsView";

afterEach(cleanup);

Object.defineProperties(HTMLElement.prototype, {
  hasPointerCapture: {
    configurable: true,
    value: () => false,
  },
  setPointerCapture: {
    configurable: true,
    value: () => {},
  },
  releasePointerCapture: {
    configurable: true,
    value: () => {},
  },
  scrollIntoView: {
    configurable: true,
    value: () => {},
  },
});

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
  it("reveals selection actions only after a transcript is selected", async () => {
    const onSetTranscriptsHidden = vi.fn().mockResolvedValue(true);
    renderVisibilityView(onSetTranscriptsHidden);

    expect(screen.queryByRole("button", { name: "Hide selected" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Copy selected transcripts" }),
    ).toBeNull();

    await userEvent.click(
      screen.getByRole("checkbox", {
        name: "Select transcript 2026-05-10 01:02:03",
      }),
    );

    expect(screen.getByRole("button", { name: "Hide selected" })).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Copy selected transcripts" }),
    ).toBeTruthy();
    expect(screen.getByText("1 selected")).toBeTruthy();
    expect(
      screen
        .getByText("Transcript selected for visibility")
        .closest(".transcript-entry")
        ?.getAttribute("aria-selected"),
    ).toBeNull();
  });

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
    expect(screen.queryByRole("button", { name: "Hide selected" })).toBeNull();
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

describe("TranscriptsView day and archive actions", () => {
  it("keeps exports visible and places secondary day actions in More", async () => {
    const onExportAllDays = vi.fn();
    const onExportDay = vi.fn();
    const onReload = vi.fn();
    render(
      <TranscriptsView
        today={new Date("2026-05-10T12:00:00+09:00")}
        days={[{ day, count: 1 }]}
        entriesByDay={new Map([[day, [entry]]])}
        openrouterKeyConfigured
        onGenerateReport={vi.fn()}
        onExportAllDays={onExportAllDays}
        onExportDay={onExportDay}
        onReload={onReload}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Export all transcript days" }),
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: "Export transcripts for 2026-05-10",
      }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "More day actions" }),
    );

    expect(screen.getByRole("menuitem", { name: "Copy all" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Summary all" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Report all" })).toBeTruthy();
    await userEvent.click(screen.getByRole("menuitem", { name: "Reload" }));

    expect(onExportAllDays).toHaveBeenCalledOnce();
    expect(onExportDay).toHaveBeenCalledWith(day);
    expect(onReload).toHaveBeenCalledWith(day);
    expect(screen.queryByText(/Autoplay next:/)).toBeNull();
  });

  it("keeps unavailable report and reload actions disabled inside More", async () => {
    render(
      <TranscriptsView
        today={new Date("2026-05-10T12:00:00+09:00")}
        days={[{ day, count: 1 }]}
        entriesByDay={new Map([[day, [entry]]])}
        loadingDay={day}
        onGenerateReport={vi.fn()}
        onReload={vi.fn()}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "More day actions" }),
    );

    expect(
      screen
        .getByRole("menuitem", { name: "Summary all" })
        .getAttribute("aria-disabled"),
    ).toBe("true");
    expect(
      screen
        .getByRole("menuitem", { name: "Report all" })
        .getAttribute("aria-disabled"),
    ).toBe("true");
    expect(
      screen
        .getByRole("menuitem", { name: "Reload" })
        .getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("announces export progress, success, and error while guarding concurrent exports", () => {
    const common = {
      today: new Date("2026-05-10T12:00:00+09:00"),
      days: [{ day, count: 1 }],
      entriesByDay: new Map([[day, [entry]]]),
      onExportAllDays: vi.fn(),
      onExportDay: vi.fn(),
    };
    const { rerender } = render(
      <TranscriptsView
        {...common}
        exportState={{ status: "running", scope: "all" }}
      />,
    );

    expect(screen.getByRole("status").textContent).toContain(
      "Exporting all transcript days",
    );
    expect(
      (
        screen.getByRole("button", {
          name: "Export all transcript days",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);

    rerender(
      <TranscriptsView
        {...common}
        onOpenExportFolder={vi.fn()}
        exportState={{
          status: "success",
          result: {
            destination_path: "/tmp/WakeNote Transcripts",
            day_count: 2,
            transcript_count: 3,
          },
        }}
      />,
    );
    expect(screen.getByRole("status").textContent).toContain(
      "Exported 3 transcripts across 2 days",
    );
    expect(
      screen.getByRole("button", { name: "Show export in Finder" }),
    ).toBeTruthy();

    rerender(
      <TranscriptsView
        {...common}
        exportState={{ status: "error", message: "Disk is full" }}
      />,
    );
    expect(screen.getByRole("status").textContent).toContain("Disk is full");
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

describe("TranscriptsView pagination and order", () => {
  it.each([30, 50, 100, 200])(
    "shows %i rows per page and resets later pages when the size changes",
    async (size) => {
      const user = userEvent.setup();
      const entries = Array.from({ length: 201 }, (_, index) => ({
        ...entry,
        transcript_path: `/tmp/WakeNote/20260510/transcript-${index}.txt`,
        recorded_at: new Date(
          Date.parse("2026-05-10T01:00:00+09:00") + index * 1000,
        ).toISOString(),
        text: `Transcript ${index + 1}`,
      }));
      const { container } = render(
        <TranscriptsView
          today={new Date("2026-05-10T12:00:00+09:00")}
          days={[{ day, count: entries.length }]}
          entriesByDay={new Map([[day, entries]])}
        />,
      );
      const visibleRows = () => container.querySelectorAll(".transcript-entry");
      expect(visibleRows()).toHaveLength(50);

      if (size === 50) {
        await user.click(
          screen.getByRole("combobox", { name: "Transcripts per page" }),
        );
        await user.click(screen.getByRole("option", { name: "30" }));
      }
      await user.click(
        screen.getByRole("button", { name: "Next Transcript page" }),
      );
      await user.click(
        screen.getByRole("combobox", { name: "Transcripts per page" }),
      );
      expect(
        screen.getAllByRole("option").map((option) => option.textContent),
      ).toEqual(["30", "50", "100", "200"]);
      await user.click(screen.getByRole("option", { name: String(size) }));

      expect(visibleRows()).toHaveLength(size);
      expect(screen.getByText(`1–${size} of 201`)).toBeTruthy();
      expect(
        screen.getByText(`Page 1 of ${Math.ceil(201 / size)}`),
      ).toBeTruthy();
      expect(visibleRows()[0].textContent).toContain("Transcript 201");
      if (size === 200) {
        await user.click(
          screen.getByRole("button", { name: "Next Transcript page" }),
        );
        expect(visibleRows()).toHaveLength(1);
        expect(screen.getByText("201–201 of 201")).toBeTruthy();
      }
    },
  );

  it("keeps the page size control available for one page and clears selection on change", async () => {
    const user = userEvent.setup();
    renderVisibilityView(vi.fn());
    await user.click(
      screen.getByRole("checkbox", {
        name: "Select transcript 2026-05-10 01:02:03",
      }),
    );
    expect(screen.getByText("1 selected")).toBeTruthy();

    await user.click(
      screen.getByRole("combobox", { name: "Transcripts per page" }),
    );
    await user.click(screen.getByRole("option", { name: "30" }));
    expect(screen.queryByText("1 selected")).toBeNull();
    expect(
      screen.queryByRole("navigation", { name: "Transcript pages" }),
    ).toBeNull();
    expect(
      screen.getByRole("combobox", { name: "Transcripts per page" })
        .textContent,
    ).toBe("30");
  });

  it("renders 50 rows per page and resets to page one when order changes", async () => {
    const user = userEvent.setup();
    const entries = Array.from({ length: 51 }, (_, index) => {
      const sequence = index + 1;
      const seconds = String(sequence).padStart(2, "0");
      return {
        ...entry,
        transcript_path: `/tmp/WakeNote/20260510/0102${seconds}.txt`,
        audio_path: `/tmp/WakeNote/20260510/0102${seconds}.m4a`,
        recorded_at: `2026-05-10T01:02:${seconds}+09:00`,
        text: `Transcript ${sequence}`,
      };
    });
    const { container } = render(
      <TranscriptsView
        today={new Date("2026-05-10T12:00:00+09:00")}
        days={[{ day, count: entries.length }]}
        entriesByDay={new Map([[day, entries]])}
      />,
    );

    const visibleTexts = () =>
      Array.from(
        container.querySelectorAll<HTMLElement>(".transcript-entry__text"),
      ).map((node) => node.textContent);
    expect(visibleTexts()).toHaveLength(50);
    expect(visibleTexts()[0]).toBe("Transcript 51");
    expect(screen.getByText("1–50 of 51")).toBeTruthy();

    await user.click(
      screen.getByRole("button", { name: "Next Transcript page" }),
    );
    expect(visibleTexts()).toEqual(["Transcript 1"]);

    await user.click(
      screen.getByRole("combobox", { name: "Transcript order" }),
    );
    await user.click(screen.getByRole("option", { name: "Oldest first" }));
    expect(screen.getByText("Page 1 of 2")).toBeTruthy();
    expect(visibleTexts()[0]).toBe("Transcript 1");
  });
});
