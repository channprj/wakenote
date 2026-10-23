// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import { StorageSettings } from "./StorageSettings";
import type { SettingsActions } from "./types";

afterEach(cleanup);

function renderStorage() {
  const onPatch = vi.fn();
  const actions = { onPatch } as unknown as SettingsActions;
  render(
    <StorageSettings
      settings={{ ...defaultSettings(), save_root: "/Archive" }}
      actions={actions}
    />,
  );
  return onPatch;
}

describe("StorageSettings", () => {
  it("turns diagnostic logs on and names the monthly log path", async () => {
    const onPatch = renderStorage();

    expect(
      screen.getByText("Append error and event logs to /Archive/logs/YYYY/MM/YYMMDD.log."),
    ).toBeTruthy();
    await userEvent.setup().click(screen.getByRole("switch", { name: "Diagnostic logs" }));

    expect(onPatch).toHaveBeenCalledWith({ diagnostic_logging_enabled: true });
  });

  it("offers to stop keeping audio files, off by default", async () => {
    const onPatch = renderStorage();
    const toggle = screen.getByRole("switch", { name: "Don't keep audio files" });

    expect(toggle.getAttribute("aria-checked")).toBe("false");
    await userEvent.setup().click(toggle);

    expect(onPatch).toHaveBeenCalledWith({ discard_transcript_audio: true });
  });
});
