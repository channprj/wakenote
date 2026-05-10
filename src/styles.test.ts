import { describe, expect, it } from "vitest";
// @ts-ignore Vitest runs this CSS source assertion in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

describe("model row styling", () => {
  it("does not add a thicker selected-state left border", () => {
    expect(css).not.toMatch(/\.model-row\[data-selected="true"\][\s\S]*?inset\s+3px\s+0\s+0/);
  });
});

describe("live transcription footer styling", () => {
  it("renders the left timestamp label as plain gray no-wrap text", () => {
    const statusRule = cssRule(".transcript-footer__status");
    const finalStatusRule = cssRule(
      '.transcript-footer__list li[data-status="final"] .transcript-footer__status',
    );

    expect(statusRule).toContain("background: transparent;");
    expect(statusRule).toContain("color: var(--muted);");
    expect(statusRule).toContain("white-space: nowrap;");
    expect(finalStatusRule).not.toMatch(/background\s*:/);
  });
});

function cssRule(selector: string) {
  const start = css.indexOf(`${selector} {`);
  expect(start, `expected CSS rule for ${selector}`).toBeGreaterThanOrEqual(0);
  const bodyStart = css.indexOf("{", start) + 1;
  const bodyEnd = css.indexOf("}", bodyStart);
  return css.slice(bodyStart, bodyEnd);
}
