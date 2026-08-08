// @ts-ignore Vitest runs this CSS source assertion in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");

function ruleBody(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = css.match(new RegExp(`${escaped}\\s*\\{([^}]*)\\}`));
  expect(match, `missing ${selector} rule`).not.toBeNull();
  return match?.[1] ?? "";
}

describe("overlay caption css", () => {
  it("does not clamp or ellipsize long caption text", () => {
    expect(css).not.toContain("-webkit-line-clamp");
    expect(css).not.toMatch(
      /\.overlay-caption__text\s*\{[^}]*overflow:\s*hidden/,
    );
    expect(css).toMatch(
      /\.overlay-caption__text\s*\{[^}]*white-space:\s*normal/,
    );
    expect(css).toMatch(
      /\.overlay-caption__text\s*\{[^}]*word-break:\s*keep-all/,
    );
    expect(css).toMatch(
      /\.overlay-caption__text\s*\{[^}]*overflow-wrap:\s*anywhere/,
    );
  });

  it("lets short captions shrink to content while enforcing the configured maximum width", () => {
    const rule = ruleBody(".overlay-caption");
    expect(rule).not.toMatch(/(^|\s)min-width\s*:/);
    expect(rule).toContain("--overlay-caption-max-width");
  });

  it("drives caption geometry and border from style variables", () => {
    const rule = ruleBody(".overlay-caption");
    for (const variable of [
      "--overlay-caption-max-width",
      "--overlay-caption-min-height",
      "--overlay-caption-max-height",
      "--overlay-caption-padding-horizontal",
      "--overlay-caption-padding-vertical",
      "--overlay-caption-border-width",
      "--overlay-caption-border-rgb",
      "--overlay-caption-border-radius",
    ]) {
      expect(rule).toContain(variable);
    }
  });

  it("keeps vertical breathing room inside the overlay window", () => {
    expect(css).toMatch(/#overlay-root\s*\{[^}]*padding:\s*8px\s+0/);
    expect(css).toMatch(/#overlay-root\s*\{[^}]*box-sizing:\s*border-box/);
    expect(ruleBody(".overlay-caption")).toContain("calc(100vh - 16px)");
  });

  it.each([
    [
      ".overlay-caption",
      "var(--overlay-caption-background-rgb, 0 0 0)",
      "var(--overlay-caption-background-alpha, 0.82)",
    ],
    [
      ".overlay-dictation",
      "var(--dictation-background-rgb, 0 0 0)",
      "var(--dictation-background-opacity, 0.88)",
    ],
  ])(
    "uses one solid variable-driven background for %s",
    (selector, rgb, opacity) => {
      const rule = ruleBody(selector);
      const normalizedRule = rule.replace(/\s+/g, " ");
      expect(rule).not.toContain("linear-gradient");
      expect(rule).not.toContain("inset");
      expect(normalizedRule).toContain(
        `background: rgb( ${rgb} / ${opacity} )`,
      );
    },
  );

  it("styles a compact dictation capsule and responsive waveform bars", () => {
    expect(css).toMatch(/\.overlay-dictation\s*\{[^}]*height:\s*44px/);
    expect(css).toMatch(/\.overlay-dictation\s*\{[^}]*gap:\s*10px/);
    expect(css).toMatch(/\.overlay-dictation\s*\{[^}]*min-width:\s*190px/);
    expect(css).toMatch(/\.overlay-dictation\s*\{[^}]*padding:\s*0 12px/);
    expect(css).not.toMatch(/backdrop-filter:\s*blur/);
    expect(css).toMatch(
      /\.overlay-dictation__waveform\s+i\s*\{[^}]*height:\s*calc\(4px \+ var\(--level\) \* 20px\)/,
    );
  });

  it("removes dictation motion when reduced motion is requested", () => {
    expect(css).toMatch(
      /@media\s*\(prefers-reduced-motion:\s*reduce\)[\s\S]*\.overlay-dictation[\s\S]*animation:\s*none/,
    );
  });

  it("supports instant, fade, and dissolve subtitle transitions", () => {
    expect(css).toMatch(
      /\.overlay-caption\[data-animation="fade"\]\[data-visibility="visible"\][^{]*\{[^}]*subtitle-fade-in/,
    );
    expect(css).toMatch(
      /\.overlay-caption\[data-animation="fade"\]\[data-visibility="exiting"\][^{]*\{[^}]*subtitle-fade-out/,
    );
    expect(css).toMatch(
      /\.overlay-caption\[data-animation="dissolve"\]\[data-visibility="visible"\][^{]*\{[^}]*subtitle-dissolve-in/,
    );
    expect(css).toMatch(
      /\.overlay-caption\[data-animation="dissolve"\]\[data-visibility="exiting"\][^{]*\{[^}]*subtitle-dissolve-out/,
    );
    expect(css).not.toMatch(
      /\.overlay-caption\[data-animation="instant"\][^{]*animation:/,
    );
  });

  it("removes subtitle motion when reduced motion is requested", () => {
    expect(css).toMatch(
      /@media\s*\(prefers-reduced-motion:\s*reduce\)[\s\S]*\.overlay-caption[\s\S]*animation:\s*none/,
    );
  });
});
