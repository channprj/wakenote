// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import type { AppSettings } from "@/lib/types";
import { GeneralSettings } from "./GeneralSettings";

afterEach(cleanup);

function renderGeneral(overrides: Partial<AppSettings> = {}) {
  const onPatch = vi.fn();
  render(<GeneralSettings settings={{ ...defaultSettings(), ...overrides }} onPatch={onPatch} />);
  return onPatch;
}

describe("GeneralSettings", () => {
  it("offers the menu bar input level, off by default", async () => {
    const onPatch = renderGeneral();
    const toggle = screen.getByRole("switch", { name: "Show input level in menu bar" });

    expect(toggle.getAttribute("aria-checked")).toBe("false");
    await userEvent.setup().click(toggle);

    expect(onPatch).toHaveBeenCalledWith({ show_menu_bar_level: true });
  });

  it("offers the dB unit while the menu bar level is shown, on by default", async () => {
    const unit = () => screen.getByRole("switch", { name: "Show dB unit" });
    renderGeneral();
    expect(unit().getAttribute("aria-checked")).toBe("true");
    expect((unit() as HTMLButtonElement).disabled).toBe(true);
    cleanup();

    const onPatch = renderGeneral({ show_menu_bar_level: true });
    expect((unit() as HTMLButtonElement).disabled).toBe(false);
    await userEvent.setup().click(unit());
    expect(onPatch).toHaveBeenCalledWith({ show_menu_bar_level_unit: false });
  });

  it("offers the colored level instead of the status dot, off by default", async () => {
    const colored = () =>
      screen.getByRole("switch", { name: "Color the level instead of the status dot" });
    renderGeneral();
    expect(colored().getAttribute("aria-checked")).toBe("false");
    expect((colored() as HTMLButtonElement).disabled).toBe(true);
    cleanup();

    const onPatch = renderGeneral({ show_menu_bar_level: true });
    expect((colored() as HTMLButtonElement).disabled).toBe(false);
    await userEvent.setup().click(colored());
    expect(onPatch).toHaveBeenCalledWith({ menu_bar_level_colored: true });
  });

  it("disables the menu bar input level while the menu bar icon is hidden", () => {
    renderGeneral({ show_tray_icon: false, show_menu_bar_level: true });

    const toggle = screen.getByRole("switch", { name: "Show input level in menu bar" });
    expect((toggle as HTMLButtonElement).disabled).toBe(true);
  });
});
