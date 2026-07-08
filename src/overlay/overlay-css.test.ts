// @ts-ignore Vitest runs this CSS source assertion in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");

describe("overlay caption css", () => {
  it("does not clamp or ellipsize long caption text", () => {
    expect(css).not.toContain("-webkit-line-clamp");
    expect(css).not.toMatch(/\.overlay-caption__text\s*\{[^}]*overflow:\s*hidden/);
    expect(css).toMatch(/\.overlay-caption__text\s*\{[^}]*white-space:\s*normal/);
  });
});
