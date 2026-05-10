import { describe, expect, it } from "vitest";
import css from "./styles.css?raw";

describe("model row styling", () => {
  it("does not add a thicker selected-state left border", () => {
    expect(css).not.toMatch(/\.model-row\[data-selected="true"\][\s\S]*?inset\s+3px\s+0\s+0/);
  });
});
