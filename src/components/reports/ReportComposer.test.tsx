// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TranscriptDay } from "@/lib/types";
import { ReportComposer } from "./ReportComposer";

const days: TranscriptDay[] = [
  { day: "2026-07-25", count: 3 },
  { day: "2026-07-27", count: 4 },
];

function renderComposer(overrides: Partial<Parameters<typeof ReportComposer>[0]> = {}) {
  const onGenerate = vi.fn();
  const onOpenChange = vi.fn();
  render(
    <ReportComposer
      days={days}
      hasActiveRun={false}
      maxIterations={3}
      model="z-ai/glm-5.2"
      onGenerate={onGenerate}
      onOpenChange={onOpenChange}
      open
      openrouterKeyConfigured
      {...overrides}
    />,
  );
  return { onGenerate, onOpenChange };
}

function generateButton() {
  return screen.getByRole("button", { name: "Generate" }) as HTMLButtonElement;
}

afterEach(cleanup);

describe("ReportComposer", () => {
  it("offers both report kinds with summary chosen by default", () => {
    renderComposer();

    const summary = screen.getByRole("radio", { name: /Summary/ }) as HTMLInputElement;
    const detailed = screen.getByRole("radio", {
      name: /Detailed report/,
    }) as HTMLInputElement;

    expect(summary.checked).toBe(true);
    expect(detailed.checked).toBe(false);
  });

  it("lists each capture day with its count, newest first", () => {
    renderComposer();

    const listed = screen
      .getAllByRole("checkbox")
      .map((checkbox) => checkbox.closest("label")?.textContent ?? "");

    expect(listed[0]).toContain("2026-07-27");
    expect(listed[0]).toContain("4 captures");
    expect(listed[1]).toContain("2026-07-25");
    expect(listed[1]).toContain("3 captures");
  });

  it("preselects the newest day and states the scope", () => {
    renderComposer();

    expect(screen.getByText("4 captures from 2026-07-27")).toBeTruthy();
  });

  it("generates the chosen kind for the selected days", async () => {
    const user = userEvent.setup();
    const { onGenerate } = renderComposer();

    await user.click(screen.getByRole("radio", { name: /Detailed report/ }));
    await user.click(screen.getByRole("checkbox", { name: /2026-07-25/ }));
    await user.click(screen.getByRole("button", { name: "Generate" }));

    expect(onGenerate).toHaveBeenCalledWith("detailed_report", [
      "2026-07-25",
      "2026-07-27",
    ]);
  });

  it("recomputes the scope as days are toggled", async () => {
    const user = userEvent.setup();
    renderComposer();

    await user.click(screen.getByRole("checkbox", { name: /2026-07-25/ }));

    expect(
      screen.getByText("7 captures across 2 days · 2026-07-25 → 2026-07-27"),
    ).toBeTruthy();
  });

  it("blocks generation without an OpenRouter key and offers Settings", async () => {
    const user = userEvent.setup();
    const onOpenIntegrationSettings = vi.fn();
    renderComposer({ openrouterKeyConfigured: false, onOpenIntegrationSettings });

    expect(generateButton().disabled).toBe(true);
    expect(screen.getByText(/OpenRouter API key/)).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Open Integrations" }));
    expect(onOpenIntegrationSettings).toHaveBeenCalled();
  });

  it("blocks generation while another run is active", () => {
    renderComposer({ hasActiveRun: true });

    expect(generateButton().disabled).toBe(true);
    expect(screen.getByText(/already being generated/)).toBeTruthy();
  });

  it("blocks generation once every day is deselected", async () => {
    const user = userEvent.setup();
    renderComposer();

    await user.click(screen.getByRole("checkbox", { name: /2026-07-27/ }));

    expect(generateButton().disabled).toBe(true);
    expect(screen.getByText(/at least one day/)).toBeTruthy();
    expect(screen.getByText("No captures selected")).toBeTruthy();
  });

  it("explains when there is nothing to report on", () => {
    renderComposer({ days: [] });

    expect(screen.getByText("No captures to report on yet")).toBeTruthy();
    expect(generateButton().disabled).toBe(true);
  });

  it("surfaces a day-loading failure instead of an empty picker", () => {
    renderComposer({ days: [], daysError: "transcript index unavailable" });

    expect(screen.getByRole("alert").textContent).toContain(
      "transcript index unavailable",
    );
  });

  it("surfaces a failure to start the run", () => {
    renderComposer({ error: "OpenRouter returned 429" });

    expect(screen.getByRole("alert").textContent).toContain(
      "OpenRouter returned 429",
    );
  });

  it("shows the model and iteration budget the run will use", () => {
    renderComposer();

    expect(screen.getByText("z-ai/glm-5.2")).toBeTruthy();
    expect(screen.getByText("Up to 3 passes")).toBeTruthy();
  });

  it("closes on Cancel", async () => {
    const user = userEvent.setup();
    const { onOpenChange } = renderComposer();

    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
