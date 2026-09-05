// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { SettingsTextEditor } from "./SettingsTextEditor";

afterEach(cleanup);

describe("settings text editing", () => {
  it("can explicitly clear an optional model override", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    render(
      <SettingsTextEditor
        label="Model"
        value="custom/model"
        allowEmpty
        multiline={false}
        onSave={save}
      />,
    );
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "" } });
    fireEvent.click(screen.getByRole("button", { name: "Save Model" }));
    await waitFor(() => expect(save).toHaveBeenCalledWith(""));
  });
  it("preserves a cleared draft and subsequent typing across snapshot updates until Save", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    const view = render(
      <SettingsTextEditor label="Prompt" value="original" onSave={save} />,
    );
    const input = screen.getByRole("textbox", {
      name: "Prompt",
    }) as HTMLTextAreaElement;
    fireEvent.change(input, { target: { value: "" } });
    view.rerender(
      <SettingsTextEditor
        label="Prompt"
        value="updated elsewhere"
        onSave={save}
      />,
    );
    expect(input.value).toBe("");
    expect(save).not.toHaveBeenCalled();
    fireEvent.change(input, {
      target: { value: "새 프롬프트\n{{transcripts}}" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save Prompt" }));
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith("새 프롬프트\n{{transcripts}}"),
    );
  });

  it("keeps a failed save editable and shows the failure", async () => {
    const save = vi.fn().mockRejectedValue(new Error("Disk is full"));
    render(
      <SettingsTextEditor label="Prompt" value="original" onSave={save} />,
    );
    fireEvent.change(screen.getByRole("textbox"), {
      target: { value: "keep this draft" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save Prompt" }));
    expect(await screen.findByRole("alert")).toHaveProperty(
      "textContent",
      "Disk is full",
    );
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(
      "keep this draft",
    );
    expect(
      (screen.getByRole("button", { name: "Save Prompt" }) as HTMLButtonElement)
        .disabled,
    ).toBe(false);
  });

  it("resets only the draft and allows cancelling back to the persisted value", () => {
    const save = vi.fn();
    render(
      <SettingsTextEditor
        label="Prompt"
        value="custom"
        defaultValue="default instructions"
        onSave={save}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: "Reset Prompt to default" }),
    );
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(
      "default instructions",
    );
    expect(save).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", { name: "Cancel Prompt changes" }),
    );
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe(
      "custom",
    );
  });

  it("prevents duplicate saves while the first save is pending", async () => {
    let finish!: () => void;
    const save = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    render(
      <SettingsTextEditor label="Prompt" value="original" onSave={save} />,
    );
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "new" } });
    const button = screen.getByRole("button", { name: "Save Prompt" });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(save).toHaveBeenCalledTimes(1);
    expect((screen.getByRole("textbox") as HTMLTextAreaElement).disabled).toBe(
      true,
    );
    await act(async () => finish());
  });
});
