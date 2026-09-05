// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import type { DictionaryFileStatus } from "@/lib/types";
import { DictionarySettingsCard } from "./DictionarySettingsCard";

afterEach(cleanup);

const syncedStatus: DictionaryFileStatus = {
  path: "/Users/test/Library/Application Support/WakeNote/dictionary.txt",
  revision: "abc123",
  error: null,
  error_line: null,
  in_sync: true,
};

function dictionarySettings() {
  return {
    ...defaultSettings(),
    dictionary: [
      {
        id: "wake-note",
        term: "WakeNote",
        aliases: ["wake note", "wake-note"],
        enabled: true,
      },
      {
        id: "qwen",
        term: "Qwen3 ASR",
        aliases: ["qwen 3 asr", "큐원 ASR"],
        enabled: false,
      },
    ],
  };
}

describe("DictionarySettingsCard", () => {
  it("preserves the open dictionary draft after persistence fails", async () => {
    const user = userEvent.setup();
    const onPatch = vi
      .fn()
      .mockRejectedValueOnce(new Error("Cannot save dictionary"))
      .mockResolvedValueOnce(undefined);
    render(
      <DictionarySettingsCard
        settings={dictionarySettings()}
        status={syncedStatus}
        onPatch={onPatch}
        onOpenFile={vi.fn()}
        onReloadFile={vi.fn()}
      />,
    );
    await user.click(
      screen.getByRole("button", { name: "Add Dictionary entry" }),
    );
    await user.type(screen.getByLabelText("Canonical term"), "Codex");
    await user.click(
      screen.getByRole("button", { name: "Save Dictionary entry" }),
    );
    expect(await screen.findByText("Cannot save dictionary")).toBeTruthy();
    expect(
      (screen.getByLabelText("Canonical term") as HTMLInputElement).value,
    ).toBe("Codex");
    await user.click(
      screen.getByRole("button", { name: "Save Dictionary entry" }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("renders compact canonical chips and saves dialog edits as one patch", async () => {
    const user = userEvent.setup();
    const onPatch = vi.fn();
    render(
      <DictionarySettingsCard
        settings={dictionarySettings()}
        status={syncedStatus}
        onPatch={onPatch}
        onOpenFile={vi.fn()}
        onReloadFile={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("button", { name: "Edit Dictionary entry WakeNote" }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Edit Dictionary entry Qwen3 ASR" }),
    ).toBeTruthy();
    expect(screen.queryByDisplayValue("wake note, wake-note")).toBeNull();

    await user.click(
      screen.getByRole("button", { name: "Edit Dictionary entry WakeNote" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Edit Dictionary entry" }),
    ).toBeTruthy();
    expect(
      (screen.getByLabelText("Canonical term") as HTMLInputElement).value,
    ).toBe("WakeNote");
    expect((screen.getByLabelText("Aliases") as HTMLInputElement).value).toBe(
      "wake note, wake-note",
    );

    fireEvent.change(screen.getByLabelText("Canonical term"), {
      target: { value: "WakeNote Pro" },
    });
    fireEvent.change(screen.getByLabelText("Aliases"), {
      target: { value: "wake note pro, WakeNote Pro" },
    });
    await user.click(
      screen.getByRole("button", { name: "Save Dictionary entry" }),
    );

    expect(onPatch).toHaveBeenCalledWith({
      dictionary: [
        {
          id: "wake-note",
          term: "WakeNote Pro",
          aliases: ["wake note pro"],
          enabled: true,
        },
        dictionarySettings().dictionary[1],
      ],
    });
  });

  it("deletes immediately and adds a new entry through an empty dialog", async () => {
    const user = userEvent.setup();
    const onPatch = vi.fn();
    render(
      <DictionarySettingsCard
        settings={dictionarySettings()}
        status={syncedStatus}
        onPatch={onPatch}
        onOpenFile={vi.fn()}
        onReloadFile={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "Delete Dictionary entry WakeNote" }),
    );
    expect(onPatch).toHaveBeenCalledWith({
      dictionary: [dictionarySettings().dictionary[1]],
    });

    await user.click(
      screen.getByRole("button", { name: "Add Dictionary entry" }),
    );
    expect(
      screen.getByRole("dialog", { name: "Add Dictionary entry" }),
    ).toBeTruthy();
    expect(
      (screen.getByLabelText("Canonical term") as HTMLInputElement).value,
    ).toBe("");
    fireEvent.change(screen.getByLabelText("Canonical term"), {
      target: { value: "Codex" },
    });
    fireEvent.change(screen.getByLabelText("Aliases"), {
      target: { value: "code x, Codex, code x" },
    });
    await user.click(
      screen.getByRole("button", { name: "Save Dictionary entry" }),
    );

    expect(onPatch).toHaveBeenLastCalledWith({
      dictionary: [
        ...dictionarySettings().dictionary,
        {
          id: "dictionary-1",
          term: "Codex",
          aliases: ["code x"],
          enabled: true,
        },
      ],
    });
  });

  it("opens dictionary.txt, shows the exact example, and exposes line errors", async () => {
    const user = userEvent.setup();
    const onOpenFile = vi.fn();
    const onReloadFile = vi.fn();
    render(
      <DictionarySettingsCard
        settings={dictionarySettings()}
        status={{
          ...syncedStatus,
          error: "line 4: alias cannot be empty",
          error_line: 4,
          in_sync: false,
        }}
        onPatch={vi.fn()}
        onOpenFile={onOpenFile}
        onReloadFile={onReloadFile}
      />,
    );

    const example = screen.getByRole("region", {
      name: "Dictionary file format example",
    });
    expect(example.textContent).toContain("WakeNote = wake note, wake-note");
    expect(example.textContent).toContain("Qwen3 ASR = qwen 3 asr, 큐원 ASR");
    expect(screen.getByRole("alert").textContent).toContain("line 4");

    await user.click(
      screen.getByRole("button", { name: "Open dictionary.txt" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Reload dictionary.txt" }),
    );
    expect(onOpenFile).toHaveBeenCalledOnce();
    expect(onReloadFile).toHaveBeenCalledOnce();
  });
});
