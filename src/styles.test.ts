import { describe, expect, it } from "vitest";
// @ts-ignore Vitest runs this CSS source assertion in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

describe("model row styling", () => {
  it("does not add a thicker selected-state left border", () => {
    // Constrain the search to the .model-row[data-selected="true"] rule body
    // (no `}` between the selector and the forbidden declaration) so unrelated
    // downstream rules that legitimately use `inset 3px 0 0` (e.g. the queue
    // row tone accents) don't trip this assertion.
    expect(css).not.toMatch(/\.model-row\[data-selected="true"\][^}]*?inset\s+3px\s+0\s+0/);
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

describe("fixed desktop shell styling", () => {
  it("uses independent desktop scroll regions instead of document scrolling", () => {
    expect(cssRule(":root")).toContain("--sidebar-width: 232px;");
    expect(cssRule("html,\nbody,\n#root")).toContain("height: 100%;");
    expect(cssRule("body")).toContain("overflow: hidden;");

    const shellRule = cssRule(".app-shell");
    expect(shellRule).toContain("grid-template-columns: var(--sidebar-width) minmax(0, 1fr);");
    expect(shellRule).toContain("height: 100vh;");
    expect(shellRule).toContain("overflow: hidden;");

    const sidebarRule = cssRule(".sidebar");
    expect(sidebarRule).toContain("height: 100vh;");
    expect(sidebarRule).toContain("overflow-y: auto;");

    const workspaceRule = cssRule(".workspace");
    expect(workspaceRule).toContain("height: 100vh;");
    expect(workspaceRule).toContain("overflow-y: auto;");
    expect(workspaceRule).toContain("overflow-x: hidden;");
    expect(workspaceRule).toContain("scrollbar-gutter: stable;");
  });

  it("aligns fixed transcript surfaces with the sidebar on desktop", () => {
    expect(cssRule(".transcript-footer")).toContain("left: var(--sidebar-width);");
    expect(cssRule(".transcript-player-sheet")).toContain("left: var(--sidebar-width);");
  });
});

describe("narrow shell styling", () => {
  it("restores document scrolling and full-width fixed transcript surfaces", () => {
    const narrowCss = cssBlock("@media (max-width: 940px)");

    expect(cssRule("html,\n  body,\n  #root", narrowCss)).toContain("height: auto;");
    expect(cssRule("body", narrowCss)).toContain("overflow: auto;");
    expect(cssRule(".app-shell", narrowCss)).toContain("height: auto;");
    expect(cssRule(".app-shell", narrowCss)).toContain("overflow: visible;");
    expect(cssRule(".sidebar", narrowCss)).toContain("height: auto;");
    expect(cssRule(".sidebar", narrowCss)).toContain("overflow-y: visible;");
    expect(cssRule(".workspace", narrowCss)).toContain("height: auto;");
    expect(cssRule(".workspace", narrowCss)).toContain("overflow: visible;");
    expect(cssRule(".transcript-footer", narrowCss)).toContain("left: 0;");
    expect(cssRule(".transcript-player-sheet", narrowCss)).toContain("left: 0;");
  });
});

function cssRule(selector: string, source = css) {
  const start = source.indexOf(`${selector} {`);
  expect(start, `expected CSS rule for ${selector}`).toBeGreaterThanOrEqual(0);
  const bodyStart = source.indexOf("{", start) + 1;
  const bodyEnd = source.indexOf("}", bodyStart);
  return source.slice(bodyStart, bodyEnd);
}

function cssBlock(selector: string) {
  const start = css.indexOf(`${selector} {`);
  expect(start, `expected CSS block for ${selector}`).toBeGreaterThanOrEqual(0);
  const bodyStart = css.indexOf("{", start) + 1;
  let depth = 1;

  for (let index = bodyStart; index < css.length; index += 1) {
    if (css[index] === "{") {
      depth += 1;
    } else if (css[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        return css.slice(bodyStart, index);
      }
    }
  }

  throw new Error(`Unclosed CSS block for ${selector}`);
}
