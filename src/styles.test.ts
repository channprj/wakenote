import { describe, expect, it } from "vitest";
// @ts-ignore Vitest runs this CSS source assertion in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";

const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const shellCss = readFileSync(new URL("./styles/shell.css", import.meta.url), "utf8");
const pagesCss = readFileSync(new URL("./styles/pages.css", import.meta.url), "utf8");

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
    expect(statusRule).toContain("color: var(--muted-foreground);");
    expect(statusRule).toContain("white-space: nowrap;");
    expect(finalStatusRule).not.toMatch(/background\s*:/);
  });
});

describe("shadcn semantic theme compatibility", () => {
  it("keeps the default dark recorder palette and an explicit light palette", () => {
    const rootRule = cssRule(":root");
    const lightRule = cssRule(":root[data-theme=\"light\"],\n[data-theme=\"light\"]");

    expect(rootRule).toContain("--background: #0b0d10;");
    expect(rootRule).toContain("--card: #15191f;");
    expect(rootRule).toContain("--primary: #60a5fa;");
    expect(rootRule).toContain("--muted: #1d232b;");
    expect(rootRule).toContain("--muted-foreground: #9aa7b8;");
    expect(lightRule).toContain("--background: #f8fafc;");
    expect(lightRule).toContain("--card: #ffffff;");
    expect(lightRule).toContain("--muted: #f1f5f9;");
  });
});

describe("fixed desktop shell styling", () => {
  it("uses independent desktop scroll regions instead of document scrolling", () => {
    expect(cssRule(":root")).toContain("--sidebar-width: 188px;");
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

describe("compact application shell styling", () => {
  it("keeps the sidebar fixed while only the page viewport scrolls", () => {
    const frameRule = cssRule(".app-frame", shellCss);
    const viewportRule = cssRule(".app-viewport", shellCss);
    const pageRule = cssRule(".app-page", shellCss);

    expect(frameRule).toContain("grid-template-columns: var(--sidebar-width) minmax(0, 1fr);");
    expect(frameRule).toContain("height: 100vh;");
    expect(frameRule).toContain("overflow: hidden;");
    expect(viewportRule).toContain("grid-template-rows: minmax(0, 1fr) auto;");
    expect(viewportRule).toContain("overflow: hidden;");
    expect(pageRule).toContain("overflow-x: hidden;");
    expect(pageRule).toContain("overflow-y: auto;");
  });
});

describe("capture recorder styling", () => {
  it("renders a bounded symmetric waveform and wraps transcript content", () => {
    const svgRule = cssRule(".recorder-waveform svg", pagesCss);
    const barRule = cssRule(".recorder-waveform__bar", pagesCss);
    const transcriptRule = cssRule(
      '[data-slot="live-transcript-list"] [data-slot="transcript-text"]',
      pagesCss,
    );

    expect(svgRule).toContain("height: 132px;");
    expect(svgRule).toContain("overflow: hidden;");
    expect(barRule).toContain("fill: currentColor;");
    expect(barRule).toContain("transition: y 80ms linear, height 80ms linear, opacity 80ms linear;");
    expect(transcriptRule).toContain("overflow-wrap: anywhere;");
    expect(pagesCss).toContain("@media (prefers-reduced-motion: reduce)");
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

describe("recording field help styling", () => {
  it("anchors desktop help within the recording control column", () => {
    const popoverRule = cssRule(".ui-field-help__popover");

    expect(popoverRule).toContain("left: 0;");
    expect(popoverRule).toContain("top: calc(100% + 8px);");
    expect(popoverRule).not.toContain("right: calc(100% + 8px);");
    expect(popoverRule).toContain("pointer-events: auto;");
    expect(popoverRule).toContain("transform: none;");
  });

  it("keeps narrow help inset from the viewport edge", () => {
    const narrowCss = cssBlock("@media (max-width: 940px)");
    const popoverRule = cssRule(".ui-field-help__popover", narrowCss);

    expect(popoverRule).toContain("left: auto;");
    expect(popoverRule).toContain("right: 16px;");
  });
});

describe("storage path pattern styling", () => {
  it("uses theme-aware high-contrast code tokens", () => {
    const rootRule = cssRule(":root");
    const lightRule = cssRule(":root[data-theme=\"light\"],\n[data-theme=\"light\"]");
    const pathPatternCodeRule = cssRule(".path-pattern code");
    const pathPatternLabelRule = cssRule(".path-pattern span,\n.offline-note span");

    expect(rootRule).toContain("--code-text: #e7edf5;");
    expect(rootRule).toContain("--code-bg: rgba(231, 237, 245, 0.08);");
    expect(lightRule).toContain("--code-text: #0f172a;");
    expect(css).toContain("code {\n  overflow-wrap: anywhere;\n  color: var(--code-text);");
    expect(pathPatternCodeRule).toContain("background: var(--code-bg);");
    expect(pathPatternCodeRule).toContain("border: 1px solid var(--code-border);");
    expect(pathPatternLabelRule).toContain("color: var(--text);");
  });
});

describe("transcript archive density", () => {
  it("keeps transcript rows compact for dense transcript lists", () => {
    const entryRule = cssRule(".transcript-entry");
    const textRule = cssRule(".transcript-entry__text");
    const selectedRule = cssRule(".transcript-entry[data-selected=\"true\"]");

    expect(entryRule).toContain("gap: 6px;");
    expect(entryRule).toContain("border-left: 2px solid transparent;");
    expect(entryRule).toContain("padding: 2px 6px 2px 4px;");
    expect(entryRule).toContain("border-radius: 0;");
    expect(selectedRule).toContain("border-left-color: var(--primary);");
    expect(selectedRule).not.toContain("box-shadow");
    expect(textRule).toContain("line-height: 1.28;");
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
