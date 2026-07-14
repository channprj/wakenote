import { describe, expect, it } from "vitest";
import { cn } from "./utils";

describe("cn", () => {
  it("keeps the final conflicting Tailwind utility", () => {
    expect(cn("px-2 text-xs", false && "hidden", "px-3")).toBe("text-xs px-3");
  });
});
