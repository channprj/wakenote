import { describe, expect, it } from "vitest";
import { defaultSettings } from "@/lib/app-state";
import {
  addMicrophonePriority,
  confirmSaveRootDisabledReason,
  formatChunkDuration,
  removeMicrophonePriority,
  reorderMicrophonePriority,
  vadGateDisabledReason,
} from "./settings-helpers";

describe("settings helpers", () => {
  const devices = [
    { id: "a", label: "Studio", available: true, fallback: false },
    { id: "b", label: "Laptop", available: true, fallback: true },
  ];

  it("adds, reorders, and removes microphone priorities without mutating input", () => {
    const original = [{ id: "a", label: "Studio" }];
    const added = addMicrophonePriority(original, devices[1]);
    const reordered = reorderMicrophonePriority(added, 1, 0);

    expect(original).toEqual([{ id: "a", label: "Studio" }]);
    expect(added.map((item) => item.id)).toEqual(["a", "b"]);
    expect(reordered.map((item) => item.id)).toEqual(["b", "a"]);
    expect(removeMicrophonePriority(reordered, 1).map((item) => item.id)).toEqual(["b"]);
    expect(removeMicrophonePriority([{ id: "a", label: "Studio" }], 0)).toEqual([
      { id: "a", label: "Studio" },
    ]);
  });

  it("keeps save-root and disabled VAD reasons exact", () => {
    expect(confirmSaveRootDisabledReason({ ...defaultSettings(), save_root: "" })).toBe(
      "Enter a save folder first",
    );
    expect(confirmSaveRootDisabledReason({ ...defaultSettings(), save_root: "/tmp" })).toBeNull();
    expect(vadGateDisabledReason()).toContain("Planned for v1");
  });

  it("formats chunk durations compactly", () => {
    expect(formatChunkDuration(60_000)).toBe("1 min");
    expect(formatChunkDuration(90_000)).toBe("90 sec");
  });
});
