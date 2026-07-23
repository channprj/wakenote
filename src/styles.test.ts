import { describe, expect, it } from "vitest";
// @ts-ignore Vitest runs this CSS source assertion in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";

const entryCss = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const tokensCss = readFileSync(new URL("./styles/tokens.css", import.meta.url), "utf8");
const shellCss = readFileSync(new URL("./styles/shell.css", import.meta.url), "utf8");
const componentsCss = readFileSync(new URL("./styles/components.css", import.meta.url), "utf8");
const pagesCss = readFileSync(new URL("./styles/pages.css", import.meta.url), "utf8");
const splitCss = [tokensCss, shellCss, componentsCss, pagesCss].join("\n");
const css = splitCss;

describe("model row styling", () => {
  it("does not add a thicker selected-state left border", () => {
    // Constrain the search to the .model-row[data-selected="true"] rule body
    // (no `}` between the selector and the forbidden declaration) so unrelated
    // downstream rules that legitimately use `inset 3px 0 0` (e.g. the queue
    // row tone accents) don't trip this assertion.
    expect(css).not.toMatch(/\.model-row\[data-selected="true"\][^}]*?inset\s+3px\s+0\s+0/);
  });
});

describe("style entrypoint", () => {
  it("owns imports only and includes every responsibility stylesheet", () => {
    expect(entryCss).toContain('@import "./styles/tokens.css";');
    expect(entryCss).toContain('@import "./styles/shell.css";');
    expect(entryCss).toContain('@import "./styles/components.css";');
    expect(entryCss).toContain('@import "./styles/pages.css";');
    expect(entryCss).not.toContain(":root {");
  });
});

describe("refined compact density tokens", () => {
  it("defines the approved spacing, type, control, radius, and motion scale", () => {
    const root = cssRule(":root", tokensCss);

    for (const declaration of [
      "--space-1: 4px;",
      "--space-2: 8px;",
      "--space-3: 12px;",
      "--space-4: 16px;",
      "--space-5: 20px;",
      "--space-6: 24px;",
      "--text-body: 12px;",
      "--text-label: 13px;",
      "--text-subtitle: 15px;",
      "--text-title: 18px;",
      "--control-compact: 32px;",
      "--control-default: 34px;",
      "--control-primary: 36px;",
      "--row-dense: 40px;",
      "--radius-control: 6px;",
      "--radius-card: 8px;",
      "--motion-fast: 140ms;",
    ]) {
      expect(root).toContain(declaration);
    }
  });

  it("keeps base layout aliases out of the component theme", () => {
    expect(tokensCss.match(/--sidebar-width:/g)).toHaveLength(1);
    expect(tokensCss.match(/--transition:/g)).toHaveLength(1);
    expect(tokensCss.match(/--radius:/g)).toHaveLength(1);
    expect(componentsCss).not.toMatch(/--sidebar-width:|--transition:|--radius:/);
  });
});

describe("active interface density contract", () => {
  const activeSelector =
    /(?:\.app-sidebar|\.page-header|\.recording-status-rail|\.recorder-|\.live-transcript|\.quick-calibration|\.settings-|\.ui-field|\.meeting-|\.transcript-|\.report-history|\.report-run|\.queue-|\.toolbar|\[data-slot="(?:live-transcript|queue-|field-row))/;
  const densityControlSelector =
    /(?:button|__row|\.meeting-row|\.transcript-entry|__autoplay|__play|__folder|source-badge|context-menu__item|ui-field-help__trigger|settings-priority-list\s+li)/;
  const activeRules = [shellCss, componentsCss, pagesCss]
    .flatMap(flatCssRules)
    .filter(({ selector }) => activeSelector.test(selector));

  it("uses typography tokens throughout active screens", () => {
    const violations = activeRules.flatMap(({ selector, body }) => {
      const declarations = cssDeclarations(body)
        .filter(
          ({ property, value }) =>
            (property === "font-size" ||
              property === "line-height") &&
            !value.includes("var(") ||
            property === "font" &&
              /\d+(?:px|rem)/.test(value),
        )
        .map(({ property, value }) => `${property}: ${value};`);
      return declarations.map(
        (declaration) => `${selector.trim()} -> ${declaration}`,
      );
    });

    expect(violations).toEqual([]);
  });

  it("uses shared height tokens for active rows and controls", () => {
    const violations = activeRules
      .filter(({ selector }) =>
        densityControlSelector.test(selector),
      )
      .flatMap(({ selector, body }) =>
        cssDeclarations(body)
          .filter(
            ({ property, value }) =>
              (property === "height" ||
                property === "min-height") &&
              /\d+(?:\.\d+)?px/.test(value) &&
              !value.includes("var("),
          )
          .map(
            ({ property, value }) =>
              `${selector.trim()} -> ${property}: ${value};`,
          ),
      );

    expect(violations).toEqual([]);
  });

  it("does not reset token-backed control typography with a font shorthand", () => {
    const nativeControlRule = cssRule(
      "button,\ninput,\nselect",
      componentsCss,
    );

    expect(nativeControlRule).toContain("font-family: inherit;");
    expect(nativeControlRule).not.toContain("font: inherit;");
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
    const root = cssRule(":root");
    expect(root).toContain("--sidebar-width-wide: 188px;");
    expect(root).toContain("--sidebar-width: var(--sidebar-width-wide);");
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

  it("switches to a 64px icon rail below the wide boundary", () => {
    const compact = cssBlock("@media (max-width: 979px)", shellCss);

    expect(compact).toContain("--sidebar-width: var(--sidebar-width-compact);");
    expect(compact).toContain(".app-sidebar__brand-copy");
    expect(compact).toContain(".app-sidebar__nav-label");
    expect(compact).toContain("display: none;");
    expect(compact).toContain("grid-template-columns: minmax(0, 1fr);");
  });

  it("does not use a resize-expensive backdrop filter in the status rail", () => {
    expect(cssRule(".recording-status-rail", shellCss)).not.toContain(
      "backdrop-filter",
    );
  });
});

describe("balanced Settings masonry", () => {
  it("uses a safe one-column fallback before measurement", () => {
    const gridRule = cssRule(".settings-grid", pagesCss);

    expect(gridRule).toContain("--masonry-row-size: 4px;");
    expect(gridRule).toContain("grid-template-columns: minmax(0, 1fr);");
  });

  it("densely packs measured cards on masonry rows", () => {
    const measuredRule = cssRule(
      '.settings-grid[data-masonry-ready="true"]',
      pagesCss,
    );

    expect(measuredRule).toContain("grid-auto-flow: dense;");
    expect(measuredRule).toContain(
      "grid-auto-rows: var(--masonry-row-size);",
    );
  });

  it("sizes shared Settings grids from their section container", () => {
    const sectionRule = cssRule(".settings-section", pagesCss);
    const twoColumn = cssBlock(
      "@container settings-section (min-width: 680px)",
      pagesCss,
    );
    const threeColumn = cssBlock(
      "@container settings-section (min-width: 1040px)",
      pagesCss,
    );

    expect(sectionRule).toContain("container: settings-section / inline-size;");
    expect(twoColumn).toContain('.settings-grid[data-max-columns="2"]');
    expect(twoColumn).toContain('.settings-grid[data-max-columns="3"]');
    expect(twoColumn).toContain(
      "grid-template-columns: repeat(2, minmax(0, 1fr));",
    );
    expect(threeColumn).toContain('.settings-grid[data-max-columns="3"]');
    expect(threeColumn).toContain(
      "grid-template-columns: repeat(3, minmax(0, 1fr));",
    );
  });
});

describe("compact overflow safety contract", () => {
  it("keeps every primary content surface inside the supported viewport", () => {
    expect(cssRule(".app-frame", splitCss)).toContain(
      "grid-template-columns: var(--sidebar-width) minmax(0, 1fr);",
    );
    expect(cssRule(".app-page", splitCss)).toContain("overflow-x: hidden;");
    expect(cssRule('[data-slot="transcript-text"]', splitCss)).toContain(
      "overflow-wrap: anywhere;",
    );
    expect(cssRule('[data-slot="queue-table"]', splitCss)).toContain(
      "table-layout: fixed;",
    );
  });

  it("keeps every Activity column participating in the table at the 980px minimum", () => {
    const activityMinimumCss = cssBlock("@media (max-width: 1020px)", pagesCss);

    expect(activityMinimumCss).not.toContain("display: none;");
  });

  it("stacks capture and meeting content before the minimum viewport", () => {
    const compactPages = cssBlock("@media (max-width: 979px)", pagesCss);
    const narrowPages = cssBlock("@media (max-width: 820px)", pagesCss);

    expect(compactPages).toContain(".meeting-progress-row");
    expect(compactPages).toContain("overflow-wrap: anywhere;");
    expect(narrowPages).toContain(".recorder-card__footer");
    expect(narrowPages).toContain("flex-direction: column;");
  });

  it("reflows transcript rows, player controls, and report history in compact mode", () => {
    const compact = cssBlock("@media (max-width: 979px)", pagesCss);

    expect(compact).toContain(".transcript-entry");
    expect(compact).toContain(
      "grid-template-columns: auto minmax(0, 1fr) auto;",
    );
    expect(compact).toContain(".transcript-player-dock audio");
    expect(compact).toContain("grid-column: 1 / -1;");
    expect(compact).toContain(".report-history__layout");
    expect(compact).toContain("grid-template-columns: minmax(0, 1fr);");
    expect(cssRule(".transcript-player-dock", pagesCss)).not.toContain(
      "backdrop-filter",
    );
  });

  it("keeps seven transcript day cells shrinkable", () => {
    const week = cssRule(".transcript-pagination__week");

    expect(week).toContain(
      "grid-template-columns: repeat(7, minmax(0, 1fr));",
    );
    expect(cssRule(".transcript-pagination__day", pagesCss)).toContain(
      "min-width: 0;",
    );
  });

  it("turns Activity rows into labeled cards and stacks Settings controls", () => {
    const compact = cssBlock("@media (max-width: 979px)", pagesCss);
    const narrow = cssBlock("@media (max-width: 820px)", pagesCss);

    expect(compact).toContain('[data-slot="queue-table"] tbody > tr');
    expect(compact).toContain("content: attr(data-label);");
    expect(compact).not.toContain(".settings-grid");
    expect(compact).toContain(".settings-inline-control");
    expect(compact).toContain(
      "grid-template-columns: minmax(0, 1fr) auto;",
    );
    expect(compact).toContain(".settings-inline-control > :first-child");
    expect(compact).toContain("scroll-padding-inline: var(--space-2);");
    expect(narrow).toContain('[data-slot="field-row"]');
    expect(narrow).toContain(".settings-slider");
    expect(narrow).toContain("grid-template-columns: minmax(0, 1fr);");
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

describe("motion accessibility", () => {
  it("disables nonessential motion when the user requests reduced motion", () => {
    const reducedMotion = cssBlock(
      "@media (prefers-reduced-motion: reduce)",
      componentsCss,
    );

    expect(reducedMotion).toContain("animation-duration: 0.01ms !important;");
    expect(reducedMotion).toContain("transition-duration: 0.01ms !important;");
  });
});

describe("record rendering performance", () => {
  it("contains offscreen record rows without duplicating their DOM", () => {
    const rowRule = cssRule(
      '.meeting-row,\n.transcript-entry,\n.report-history__row,\n[data-slot="queue-table"] tbody > tr:not(.table-group-row)',
      pagesCss,
    );

    expect(rowRule).toContain("content-visibility: auto;");
    expect(rowRule).toContain(
      "contain-intrinsic-size: auto var(--row-dense);",
    );
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
    expect(textRule).toContain("line-height: var(--leading-body);");
  });
});

function cssRule(selector: string, source = css) {
  const start = source.indexOf(`${selector} {`);
  expect(start, `expected CSS rule for ${selector}`).toBeGreaterThanOrEqual(0);
  const bodyStart = source.indexOf("{", start) + 1;
  const bodyEnd = source.indexOf("}", bodyStart);
  return source.slice(bodyStart, bodyEnd);
}

function cssBlock(selector: string, source = css) {
  const start = source.indexOf(`${selector} {`);
  expect(start, `expected CSS block for ${selector}`).toBeGreaterThanOrEqual(0);
  const bodyStart = source.indexOf("{", start) + 1;
  let depth = 1;

  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") {
      depth += 1;
    } else if (source[index] === "}") {
      depth -= 1;
      if (depth === 0) {
        return source.slice(bodyStart, index);
      }
    }
  }

  throw new Error(`Unclosed CSS block for ${selector}`);
}

function flatCssRules(source: string) {
  return [...source.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map(
    ([, selector, body]) => ({ selector, body }),
  );
}

function cssDeclarations(body: string) {
  return body
    .split(";")
    .map((declaration) => declaration.trim())
    .filter(Boolean)
    .flatMap((declaration) => {
      const separator = declaration.indexOf(":");
      if (separator < 0) return [];
      return [{
        property: declaration.slice(0, separator).trim(),
        value: declaration.slice(separator + 1).trim(),
      }];
    });
}
