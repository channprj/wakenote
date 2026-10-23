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
});
