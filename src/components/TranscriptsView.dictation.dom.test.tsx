// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RecentTranscript } from "@/lib/types";
import { TranscriptsView } from "./TranscriptsView";

afterEach(cleanup);
Object.defineProperties(HTMLElement.prototype, {
  hasPointerCapture: { configurable: true, value: () => false },
  setPointerCapture: { configurable: true, value: () => {} },
  releasePointerCapture: { configurable: true, value: () => {} },
  scrollIntoView: { configurable: true, value: () => {} },
});
const day = "2026-05-10";
const previousDay = "2026-05-09";
const today = new Date("2026-05-10T12:00:00+09:00");
const microphone: RecentTranscript = {
  transcript_path: "/tmp/microphone.txt",
  audio_path: null,
  recorded_at: "2026-05-10T09:00:00+09:00",
  text: "Ordinary microphone transcript",
  source: "microphone",
};
const dictation: RecentTranscript = {
  ...microphone,
  transcript_path: "/tmp/dictation.txt",
  text: "Only the dictated words",
  source_label: "dictation",
};

describe("Dictation history filtering", () => {
  it("filters to dictations and retains the filter on a day without dictations", async () => {
    const user = userEvent.setup();
    render(
      <TranscriptsView
        today={today}
        days={[
          { day, count: 2 },
          { day: previousDay, count: 1 },
        ]}
        entriesByDay={
          new Map([
            [day, [microphone, dictation]],
            [previousDay, [microphone]],
          ])
        }
      />,
    );
    await user.click(
      screen.getByRole("combobox", { name: "Transcript source" }),
    );
    await user.click(screen.getByRole("option", { name: "Dictations (1)" }));
    expect(screen.getByText(dictation.text)).toBeTruthy();
    expect(screen.queryByText(microphone.text)).toBeNull();
    await user.click(screen.getByRole("button", { name: "Previous week" }));
    await user.click(
      screen.getByRole("button", { name: `Go to ${previousDay} transcripts` }),
    );
    expect(screen.getByText("No dictations for this day")).toBeTruthy();
    expect(
      screen.getByRole("combobox", { name: "Transcript source" }).textContent,
    ).toContain("Dictations (0)");
    expect(screen.queryByText(microphone.text)).toBeNull();
    await user.click(
      screen.getByRole("combobox", { name: "Transcript source" }),
    );
    await user.click(screen.getByRole("option", { name: "All sources (1)" }));
    expect(screen.getByText(microphone.text)).toBeTruthy();
  });

  it("preserves a recent dictation's day and filter while the archive loads", () => {
    const onActiveDayChange = vi.fn();
    const view = render(
      <TranscriptsView
        today={today}
        days={[]}
        entriesByDay={new Map()}
        initialSourceFilter="dictation"
        initialActiveDay={previousDay}
        onActiveDayChange={onActiveDayChange}
      />,
    );
    expect(onActiveDayChange).toHaveBeenCalledWith(previousDay);
    expect(onActiveDayChange).not.toHaveBeenCalledWith(day);
    expect(
      screen.getByRole("combobox", { name: "Transcript source" }).textContent,
    ).toContain("Dictations (0)");
    view.rerender(
      <TranscriptsView
        today={today}
        days={[{ day: previousDay, count: 2 }]}
        entriesByDay={new Map([[previousDay, [dictation, microphone]]])}
        initialSourceFilter="dictation"
        initialActiveDay={previousDay}
        onActiveDayChange={onActiveDayChange}
      />,
    );
    expect(screen.getByText(dictation.text)).toBeTruthy();
    expect(screen.queryByText(microphone.text)).toBeNull();
  });
});
