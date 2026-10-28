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
  it("keeps paged captions within a three-row safety boundary", () => {
    const rule = ruleBody(".overlay-caption__text");
    expect(rule).toContain("white-space: pre-line");
    expect(rule).toContain("word-break: keep-all");
    expect(rule).toContain("overflow-wrap: anywhere");
    expect(rule).toContain("max-height: calc(1.25em * 3)");
    expect(rule).toContain("overflow: hidden");
    expect(rule).not.toContain("text-overflow: ellipsis");
    expect(css).not.toContain("-webkit-line-clamp");
  });

  it("drives caption geometry and border from style variables", () => {
    const rule = ruleBody(".overlay-caption");
    // Short captions shrink to content; only the configured maximum width bounds them.
    expect(rule).not.toMatch(/(^|\s)min-width\s*:/);
    for (const variable of [
      "--overlay-caption-max-width",
      "--overlay-caption-max-height",
      "--overlay-caption-padding-horizontal",
      "--overlay-caption-padding-vertical",
    ]) {
      expect(rule).toContain(variable);
    }
    expect(rule).not.toContain("--overlay-caption-min-height");
    expect(rule).toContain("background: transparent");
    expect(rule).not.toContain("box-shadow: 0");
  });

  it("keeps vertical breathing room inside the overlay window", () => {
    expect(css).toMatch(/#overlay-root\s*\{[^}]*padding:\s*8px\s+0/);
    expect(css).toMatch(/#overlay-root\s*\{[^}]*box-sizing:\s*border-box/);
    expect(ruleBody(".overlay-caption")).toContain("calc(100vh - 16px)");
  });

  it("outlines caption letters and keeps one solid dictation background", () => {
    const caption = ruleBody(".overlay-caption");
    const text = ruleBody(".overlay-caption__text");
    expect(caption).toContain("background: transparent");
    expect(caption).not.toContain("linear-gradient");
    expect(text).toContain(
      "-webkit-text-stroke-color: var(--overlay-caption-outline, #000)",
    );
    expect(text).toContain(
      "-webkit-text-stroke-width: var(--overlay-caption-outline-width, 2px)",
    );
    expect(text).toContain("paint-order: stroke fill");
    for (const [selector, rgb, opacity] of [
      [
        ".overlay-dictation",
        "var(--dictation-background-rgb, 0 0 0)",
        "var(--dictation-background-opacity, 0.88)",
      ],
    ]) {
      const rule = ruleBody(selector);
      const normalizedRule = rule.replace(/\s+/g, " ");
      expect(rule, selector).not.toContain("linear-gradient");
      expect(rule, selector).not.toContain("inset");
      expect(normalizedRule, selector).toContain(
        `background: rgb( ${rgb} / ${opacity} )`,
      );
    }
  });

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

  it("removes caption and dictation motion when reduced motion is requested", () => {
    for (const selector of [".overlay-dictation", ".overlay-caption"]) {
      const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      expect(css, selector).toMatch(
        new RegExp(
          `@media\\s*\\(prefers-reduced-motion:\\s*reduce\\)[\\s\\S]*${escaped}[\\s\\S]*animation:\\s*none`,
        ),
      );
    }
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
});
