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

  it("keeps vertical breathing room inside the overlay window", () => {
    expect(css).toMatch(/#overlay-root\s*\{[^}]*padding:\s*8px\s+0/);
    expect(css).toMatch(/#overlay-root\s*\{[^}]*box-sizing:\s*border-box/);
    expect(css).toMatch(/\.overlay-caption\s*\{[^}]*max-height:\s*calc\(100vh - 16px\)/);
  });
});
