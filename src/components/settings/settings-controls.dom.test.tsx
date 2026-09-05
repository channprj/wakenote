// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingNumberInput, SettingSlider } from "./settings-controls";

vi.mock("@/components/ui/slider", () => ({
  Slider: ({
    value,
    onValueChange,
    onValueCommit,
  }: {
    value: number[];
    onValueChange: (value: number[]) => void;
    onValueCommit: (value: number[]) => void;
  }) => (
    <input
      aria-label="Slider"
      type="range"
      value={value[0]}
      onChange={(event) => onValueChange([Number(event.target.value)])}
      onPointerUp={(event) =>
        onValueCommit([Number(event.currentTarget.value)])
      }
    />
  ),
}));
afterEach(cleanup);

describe("settings input commits", () => {
  it("does not save a cleared or canceled number and commits Enter exactly once", async () => {
    const user = userEvent.setup();
    const save = vi.fn();
    render(
      <SettingNumberInput
        label="Size"
        value={20}
        min={0}
        max={100}
        onValueChange={save}
      />,
    );
    const input = screen.getByRole("spinbutton");
    await user.clear(input);
    await user.tab();
    expect((input as HTMLInputElement).value).toBe("20");
    expect(save).not.toHaveBeenCalled();
    await user.clear(input);
    await user.type(input, "42{Escape}");
    expect(save).not.toHaveBeenCalled();
    expect((input as HTMLInputElement).value).toBe("20");
    await user.clear(input);
    await user.type(input, "42{Enter}");
    expect(save).toHaveBeenCalledExactlyOnceWith(42);
  });

  it("shows a slider draft without persisting every movement", () => {
    const save = vi.fn();
    render(
      <SettingSlider
        label="Size"
        value={20}
        min={0}
        max={100}
        onValueChange={save}
      />,
    );
    const input = screen.getByRole("slider");
    fireEvent.change(input, { target: { value: "30" } });
    fireEvent.change(input, { target: { value: "40" } });
    expect(screen.getByText("40")).toBeTruthy();
    expect(save).not.toHaveBeenCalled();
    fireEvent.pointerUp(input);
    expect(save).toHaveBeenCalledExactlyOnceWith(40);
  });
});
