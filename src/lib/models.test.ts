import { describe, expect, it } from "vitest";
import { formatModelLabel } from "./models";
import type { ModelDescriptor } from "./types";

function model(id: string, displayName: string): Pick<ModelDescriptor, "id" | "display_name"> {
  return { id, display_name: displayName };
}

describe("formatModelLabel", () => {
  it("returns the friendly display name when the model is in the list", () => {
    const models = [model("whisper-medium", "Whisper Medium"), model("whisper-small", "Whisper Small")];

    expect(formatModelLabel("whisper-medium", models)).toBe("Whisper Medium");
    expect(formatModelLabel("whisper-small", models)).toBe("Whisper Small");
  });

  it("falls back to the raw id when the model is not in the list", () => {
    const models = [model("whisper-medium", "Whisper Medium")];

    expect(formatModelLabel("whisper-removed", models)).toBe("whisper-removed");
  });

  it("falls back to the raw id when the matched display name is blank", () => {
    const models = [model("whisper-blank", "   ")];

    expect(formatModelLabel("whisper-blank", models)).toBe("whisper-blank");
  });

  it("handles an empty model list", () => {
    expect(formatModelLabel("whisper-medium", [])).toBe("whisper-medium");
  });
});
