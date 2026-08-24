// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ListVisibilityToolbar } from "./ListVisibilityToolbar";

afterEach(cleanup);

function renderToolbar(
  overrides: Partial<React.ComponentProps<typeof ListVisibilityToolbar>> = {},
) {
  const props: React.ComponentProps<typeof ListVisibilityToolbar> = {
    mode: "visible",
    visibleCount: 3,
    hiddenCount: 2,
    selectedCount: 2,
    totalInMode: 3,
    mutating: false,
    statusMessage: "",
    onModeChange: vi.fn(),
    onSelectAll: vi.fn(),
    onClearSelection: vi.fn(),
    onApplySelection: vi.fn(),
    ...overrides,
  };
  render(<ListVisibilityToolbar {...props} />);
  return props;
}

describe("ListVisibilityToolbar", () => {
  it("shows mode counts and applies Hide in Visible mode", async () => {
    const props = renderToolbar();

    expect(screen.getByRole("radio", { name: /Visible3/ })).toBeTruthy();
    expect(screen.getByRole("radio", { name: /Hidden2/ })).toBeTruthy();

    await userEvent.click(
      screen.getByRole("button", { name: "Hide selected" }),
    );
    expect(props.onApplySelection).toHaveBeenCalledOnce();
  });

  it("offers Restore in Hidden mode and delegates the tab change", async () => {
    const props = renderToolbar({ mode: "hidden" });

    expect(
      screen.getByRole("button", { name: "Restore selected" }),
    ).toBeTruthy();
    await userEvent.click(screen.getByRole("radio", { name: /Visible3/ }));

    expect(props.onModeChange).toHaveBeenCalledWith("visible");
  });

  it("selects all or clears through the shared checkbox", async () => {
    const selectProps = renderToolbar({
      selectedCount: 0,
    });
    await userEvent.click(
      screen.getByRole("checkbox", { name: "Select all items" }),
    );
    expect(selectProps.onSelectAll).toHaveBeenCalledOnce();
    cleanup();

    const clearProps = renderToolbar({
      selectedCount: 3,
    });
    await userEvent.click(
      screen.getByRole("checkbox", { name: "Clear selection" }),
    );
    expect(clearProps.onClearSelection).toHaveBeenCalledOnce();
  });

  it("disables mutations and announces status politely", () => {
    renderToolbar({
      mutating: true,
      statusMessage: "Hidden from list · Files remain on disk",
    });

    expect(
      (
        screen.getByRole("button", {
          name: "Hide selected",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(true);
    expect(screen.getByRole("status").getAttribute("aria-live")).toBe("polite");
  });

  it("keeps only the visibility switch when selection controls live externally", () => {
    renderToolbar({ selectionPlacement: "external" });

    expect(screen.getByRole("radio", { name: /Visible3/ })).toBeTruthy();
    expect(screen.getByRole("radio", { name: /Hidden2/ })).toBeTruthy();
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByRole("button", { name: "Hide selected" })).toBeNull();
  });
});
