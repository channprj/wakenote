// @vitest-environment jsdom
import { useState } from "react";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TranscriptionLanguage } from "@/lib/types";
import { LanguageHintsControl } from "./LanguageHintsControl";

afterEach(cleanup);
Object.defineProperties(HTMLElement.prototype, {
  hasPointerCapture: { configurable: true, value: () => false },
  setPointerCapture: { configurable: true, value: () => {} },
  releasePointerCapture: { configurable: true, value: () => {} },
  scrollIntoView: { configurable: true, value: () => {} },
});

function Harness({
  save,
}: {
  save: (value: TranscriptionLanguage[]) => void | Promise<void>;
}) {
  const [value, setValue] = useState<TranscriptionLanguage[]>(["en", "ko"]);
  return (
    <LanguageHintsControl
      value={value}
      onChange={async (next) => {
        await save(next);
        setValue(next);
      }}
    />
  );
}

describe("LanguageHintsControl", () => {
  it("selects multiple hints and can clear them for automatic detection", async () => {
    const user = userEvent.setup();
    const save = vi.fn();
    render(<Harness save={save} />);
    const trigger = screen.getByRole("button", {
      name: "Transcription language hints",
    });
    expect(trigger.textContent).toBe("English, Korean");
    await user.click(trigger);
    expect(
      screen
        .getByRole("menuitemcheckbox", { name: "English" })
        .getAttribute("aria-checked"),
    ).toBe("true");
    expect(
      screen
        .getByRole("menuitemcheckbox", { name: "Korean" })
        .getAttribute("aria-checked"),
    ).toBe("true");
    await user.click(screen.getByRole("menuitemcheckbox", { name: "English" }));
    await user.click(
      screen.getByRole("menuitemcheckbox", { name: "Japanese" }),
    );
    expect(save).toHaveBeenLastCalledWith(["ko", "ja"]);
    await user.click(
      screen.getByRole("menuitem", { name: "Auto-detect (no hints)" }),
    );
    expect(save).toHaveBeenLastCalledWith([]);
    expect(trigger.textContent).toBe("Auto-detect");
  });

  it("waits for a pending save so the next selection includes the saved hints", async () => {
    const user = userEvent.setup();
    let finish!: () => void;
    const pending = new Promise<void>((resolve) => {
      finish = resolve;
    });
    const save = vi
      .fn()
      .mockReturnValueOnce(pending)
      .mockResolvedValue(undefined);
    render(<Harness save={save} />);
    await user.click(
      screen.getByRole("button", { name: "Transcription language hints" }),
    );
    await user.click(
      screen.getByRole("menuitemcheckbox", { name: "Japanese" }),
    );
    expect(
      screen
        .getByRole("menuitemcheckbox", { name: "German" })
        .getAttribute("aria-disabled"),
    ).toBe("true");
    await act(async () => finish());
    await user.click(screen.getByRole("menuitemcheckbox", { name: "German" }));
    expect(save).toHaveBeenLastCalledWith(["en", "ko", "ja", "de"]);
  });
});
