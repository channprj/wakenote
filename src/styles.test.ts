import { describe, expect, it } from "vitest";
// @ts-ignore Vitest runs this CSS source assertion in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";

const entryCss = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const tokensCss = readFileSync(
  new URL("./styles/tokens.css", import.meta.url),
  "utf8",
);
const shellCss = readFileSync(
  new URL("./styles/shell.css", import.meta.url),
  "utf8",
);
const componentsCss = readFileSync(
  new URL("./styles/components.css", import.meta.url),
  "utf8",
);
const pagesCss = readFileSync(
  new URL("./styles/pages.css", import.meta.url),
  "utf8",
);
const splitCss = [tokensCss, shellCss, componentsCss, pagesCss].join("\n");
const css = splitCss;

describe("model row styling", () => {
  it("does not add a thicker selected-state left border", () => {
    // Constrain the search to the .model-row[data-selected="true"] rule body
    // (no `}` between the selector and the forbidden declaration) so unrelated
    // downstream rules that legitimately use `inset 3px 0 0` (e.g. the queue
    // row tone accents) don't trip this assertion.
    expect(css).not.toMatch(
      /\.model-row\[data-selected="true"\][^}]*?inset\s+3px\s+0\s+0/,
    );
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
    expect(componentsCss).not.toMatch(
      /--sidebar-width:|--transition:|--radius:/,
    );
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
            ((property === "font-size" || property === "line-height") &&
              !value.includes("var(")) ||
            (property === "font" && /\d+(?:px|rem)/.test(value)),
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
      .filter(({ selector }) => densityControlSelector.test(selector))
      .flatMap(({ selector, body }) =>
        cssDeclarations(body)
          .filter(
            ({ property, value }) =>
              (property === "height" || property === "min-height") &&
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
    const nativeControlRule = cssRule("button,\ninput,\nselect", componentsCss);

    expect(nativeControlRule).toContain("font-family: inherit;");
    expect(nativeControlRule).not.toContain("font: inherit;");
  });
});

describe("shadcn semantic theme compatibility", () => {
  it("keeps the default dark recorder palette and an explicit light palette", () => {
    const rootRule = cssRule(":root");
    const lightRule = cssRule(
      ':root[data-theme="light"],\n[data-theme="light"]',
    );

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
    expect(shellRule).toContain(
      "grid-template-columns: var(--sidebar-width) minmax(0, 1fr);",
    );
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

    expect(frameRule).toContain(
      "grid-template-columns: var(--sidebar-width) minmax(0, 1fr);",
    );
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

    expect(gridRule).toContain("--masonry-row-size: 1px;");
    expect(gridRule).toContain("--masonry-card-gap: var(--page-gap);");
    expect(gridRule).toContain("gap: var(--masonry-card-gap);");
    expect(gridRule).toContain("grid-template-columns: minmax(0, 1fr);");
  });

  it("densely packs measured cards on masonry rows", () => {
    const measuredRule = cssRule(
      '.settings-grid[data-masonry-ready="true"]',
      pagesCss,
    );

    expect(measuredRule).toContain("grid-auto-flow: dense;");
    expect(measuredRule).toContain("grid-auto-rows: var(--masonry-row-size);");
    expect(measuredRule).toContain("row-gap: 0;");
    expect(measuredRule).toContain("column-gap: var(--masonry-card-gap);");
  });

  it("only expands measured Settings grids from their section container", () => {
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
    expect(twoColumn).toContain(
      '.settings-grid[data-masonry-ready="true"][data-max-columns="2"]',
    );
    expect(twoColumn).toContain(
      '.settings-grid[data-masonry-ready="true"][data-max-columns="3"]',
    );
    expect(twoColumn).toContain(
      "grid-template-columns: repeat(2, minmax(0, 1fr));",
    );
    expect(threeColumn).toContain(
      '.settings-grid[data-masonry-ready="true"][data-max-columns="3"]',
    );
    expect(threeColumn).toContain(
      "grid-template-columns: repeat(3, minmax(0, 1fr));",
    );
    expect(twoColumn).not.toContain(".settings-grid[data-max-columns=");
    expect(threeColumn).not.toContain(".settings-grid[data-max-columns=");
  });

  it("keeps system-audio content globally shrinkable", () => {
    const narrowCardStart = pagesCss.indexOf(
      "@container settings-card (max-width: 360px)",
    );
    expect(narrowCardStart).toBeGreaterThan(0);
    const globalCardCss = pagesCss.slice(0, narrowCardStart);
    const shrinkableRule = cssRule(
      ".system-audio-settings,\n.system-audio-banner",
      globalCardCss,
    );
    const bannerContentRule = cssRule(
      ".system-audio-banner > span",
      globalCardCss,
    );

    expect(shrinkableRule).toContain("min-width: 0;");
    expect(bannerContentRule).toContain("min-width: 0;");
    expect(bannerContentRule).toContain("flex: 1 1 180px;");
    expect(bannerContentRule).toContain("overflow-wrap: anywhere;");
    expect(bannerContentRule).toContain("white-space: normal;");
  });

  it("keeps only responsive alignment and wrapping inside narrow cards", () => {
    const cardRule = cssRule(".settings-card", pagesCss);
    const narrowCard = cssBlock(
      "@container settings-card (max-width: 360px)",
      pagesCss,
    );
    const inlineRule = cssRule(".settings-inline-control", narrowCard);
    const firstChildRule = cssRule(
      ".settings-inline-control > :first-child",
      narrowCard,
    );
    const bannerRule = cssRule(".system-audio-banner", narrowCard);

    expect(cardRule).toContain("container: settings-card / inline-size;");
    expect(inlineRule).toContain("grid-template-columns: minmax(0, 1fr) auto;");
    expect(firstChildRule).toContain("grid-column: 1 / -1;");
    expect(bannerRule).toContain("align-items: flex-start;");
    expect(bannerRule).toContain("flex-wrap: wrap;");
    expect(bannerRule).not.toContain("min-width:");
    expect(bannerRule).not.toContain("white-space:");
    expect(narrowCard).not.toContain(".system-audio-settings");
    expect(narrowCard).not.toContain(".system-audio-banner > span");
  });

  it("stacks Model files above the model collection without width overflow", () => {
    const stackRule = cssRule(".models-settings", pagesCss);
    const childRule = cssRule(
      '.models-settings > *,\n.models-settings [data-slot="model-files-card"]',
      pagesCss,
    );

    expect(stackRule).toContain("display: grid;");
    expect(stackRule).toContain("grid-template-columns: minmax(0, 1fr);");
    expect(stackRule).toContain("min-width: 0;");
    expect(stackRule).toContain("gap: var(--page-gap);");
    expect(childRule).toContain("min-width: 0;");
  });

  it("keeps model cards in a safe one-column fallback until measurement", () => {
    const listRule = cssRule(".model-list", componentsCss);
    const childRule = cssRule(".model-list > *", componentsCss);
    const measuredRule = cssRule(
      '.model-list[data-masonry-ready="true"]',
      componentsCss,
    );

    expect(listRule).toContain("--masonry-row-size: 1px;");
    expect(listRule).toContain("--masonry-card-gap: var(--page-gap);");
    expect(listRule).toContain("gap: var(--masonry-card-gap);");
    expect(listRule).toContain("display: grid;");
    expect(listRule).toContain("grid-template-columns: minmax(0, 1fr);");
    expect(listRule).toContain("align-items: start;");
    expect(childRule).toContain("min-width: 0;");
    expect(childRule).toContain("align-self: start;");
    expect(measuredRule).toContain("grid-auto-flow: dense;");
    expect(measuredRule).toContain("grid-auto-rows: var(--masonry-row-size);");
    expect(measuredRule).toContain("row-gap: 0;");
    expect(measuredRule).toContain("column-gap: var(--masonry-card-gap);");
  });

  it("only upgrades measured model collections to two or three columns", () => {
    const twoColumn = cssBlock(
      "@container settings-section (min-width: 680px)",
      pagesCss,
    );
    const threeColumn = cssBlock(
      "@container settings-section (min-width: 1040px)",
      pagesCss,
    );
    const modelRules = flatCssRules(`${componentsCss}\n${pagesCss}`).filter(
      ({ selector }) => selector.includes(".model-list"),
    );

    expect(twoColumn).toContain(
      '.model-list[data-masonry-ready="true"][data-max-columns="2"]',
    );
    expect(twoColumn).toContain(
      '.model-list[data-masonry-ready="true"][data-max-columns="3"]',
    );
    expect(twoColumn).toContain(
      "grid-template-columns: repeat(2, minmax(0, 1fr));",
    );
    expect(threeColumn).toContain(
      '.model-list[data-masonry-ready="true"][data-max-columns="3"]',
    );
    expect(threeColumn).toContain(
      "grid-template-columns: repeat(3, minmax(0, 1fr));",
    );
    expect(twoColumn).not.toContain(".model-list[data-max-columns=");
    expect(threeColumn).not.toContain(".model-list[data-max-columns=");

    for (const { selector, body } of modelRules) {
      if (
        body.includes("grid-auto-flow: dense;") ||
        body.includes("grid-auto-rows:") ||
        body.includes("grid-template-columns: repeat(")
      ) {
        expect(selector).toContain('[data-masonry-ready="true"]');
      }
      expect(selector).not.toContain('[data-max-columns="4"]');
      expect(body).not.toContain("repeat(4,");
    }
  });

  it("uses vertical model cards with safely wrapping metadata and actions", () => {
    const cardRule = cssRule(".model-row", componentsCss);
    const mainRule = cssRule(
      ".model-row__main,\n.model-row__main header",
      componentsCss,
    );
    const wrappingRule = cssRule(
      ".model-row__main header > div,\n.model-row__scores span",
      componentsCss,
    );
    const scoresRule =
      flatCssRules(componentsCss).find(
        ({ selector }) => selector.trim() === ".model-row__scores",
      )?.body ?? "";
    const actionsRule =
      flatCssRules(componentsCss).find(
        ({ selector }) => selector.trim() === ".model-row__actions",
      )?.body ?? "";
    const sharedActionsRule = cssRule(
      ".status-strip,\n.toolbar,\n.row-actions,\n.section__actions,\n.model-row__actions",
      componentsCss,
    );
    const dangerRule = cssRule(
      '.model-row__scores span[data-tone="danger"]',
      componentsCss,
    );

    expect(cardRule).toContain("grid-template-columns: minmax(0, 1fr);");
    expect(mainRule).toContain("min-width: 0;");
    expect(wrappingRule).toContain("min-width: 0;");
    expect(wrappingRule).toContain("overflow-wrap: anywhere;");
    expect(scoresRule).toContain("flex-wrap: wrap;");
    expect(sharedActionsRule).toContain("flex-wrap: wrap;");
    expect(actionsRule).toContain("min-width: 0;");
    expect(actionsRule).toContain("justify-content: flex-start;");
    expect(dangerRule).toContain("flex-basis: 100%;");
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

  it("reserves enough desktop Activity width for every running-job action", () => {
    const actionsColumn = cssRule(
      '[data-slot="queue-table"] th:nth-child(5),\n.queue-job__actions-cell',
      pagesCss,
    );

    // Six 34px icon controls, five 2px gaps, and 8px cell padding per side.
    expect(actionsColumn).toContain("width: 230px;");
  });

  it("keeps compact Activity values out of the narrow label column", () => {
    const compact = cssBlock("@media (max-width: 979px)", pagesCss);
    const values = cssRule(
      '[data-slot="queue-table"] tbody > tr:not(.table-group-row) > td:not(.empty-cell) > *',
      compact,
    );
    const group = cssRule(
      '[data-slot="queue-table"] .table-group-row > td',
      compact,
    );

    expect(values).toContain("grid-column: 2;");
    expect(group).toContain("display: block;");
    expect(group).toContain("width: 100%;");
    expect(group).toContain("max-width: none;");
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
    expect(cssRule(".activity-audio-player", pagesCss)).toContain(
      "grid-template-columns: minmax(120px, 1fr) minmax(210px, 320px) auto;",
    );
    expect(cssRule(".activity-audio-player", compact)).toContain(
      "grid-template-columns: minmax(0, 1fr) auto;",
    );
    expect(compact).toContain(".report-history__layout");
    expect(compact).toContain("grid-template-columns: minmax(0, 1fr);");
    expect(cssRule(".transcript-player-dock", pagesCss)).not.toContain(
      "backdrop-filter",
    );
  });

  it("stacks the report toolbar's filter, search, and actions in compact mode", () => {
    const compact = cssBlock("@media (max-width: 979px)", pagesCss);
    const wideToolbar = cssRule(".report-history__toolbar", componentsCss);
    const wideSearch = cssRule(".report-history__search", componentsCss);

    // Wide: kind filter | search (absorbs slack) | actions.
    expect(wideToolbar).toContain(
      "grid-template-columns: auto minmax(0, 1fr) auto;",
    );
    expect(wideSearch).toContain("justify-self: end;");

    // Compact: one column, with the search spanning it instead of being pinned right.
    expect(cssRule(".report-history__toolbar", compact)).toContain(
      "grid-template-columns: minmax(0, 1fr);",
    );
    expect(cssRule(".report-history__search", compact)).toContain(
      "justify-self: stretch;",
    );
    expect(cssRule(".report-history__toolbar-actions", compact)).toContain(
      "justify-content: flex-start;",
    );
  });

  it("gives workspace pages the viewport height and the slack to their content", () => {
    const workspace = cssRule(".primary-workspace", shellCss);
    const lastChild = cssRule(".primary-workspace > :last-child", shellCss);
    const reportLayout = cssRule(".report-history__layout", componentsCss);
    const reportList = cssRule(".report-history__list", componentsCss);
    const queuePanel = cssRule(".queue-panel", componentsCss);

    expect(workspace).toContain("min-height: 100%;");
    expect(workspace).toContain("flex-direction: column;");
    expect(lastChild).toContain("flex: 1 1 auto;");
    expect(lastChild).toContain("min-height: 0;");

    // The report panes grow instead of being pinned to a fixed height.
    expect(reportLayout).toContain("flex: 1 1 auto;");
    expect(reportLayout).not.toMatch(/min-height:\s*\d+px/);
    expect(reportList).not.toContain("max-height:");

    // Only the job table absorbs slack; stats, toolbar, date picker, and pagination keep their own height.
    expect(queuePanel).toContain(
      "grid-template-rows: auto auto auto auto auto minmax(0, 1fr) auto;",
    );
    expect(cssRule(".queue-view-bar", componentsCss)).toContain(
      "display: flex;",
    );

    // Naturally short panels stay top-aligned rather than stretching rows.
    expect(cssRule(".meeting-panel", componentsCss)).toContain(
      "align-content: start;",
    );
    expect(cssRule(".transcripts-panel", componentsCss)).toContain(
      "align-content: start;",
    );
  });

  it("lays out meeting detail as a responsive progress rail and live transcript desk", () => {
    const workspace = cssRule(".meeting-detail__workspace", pagesCss);
    const document = cssRule(".meeting-detail__document", pagesCss);
    const liveEdge = cssRule(".meeting-detail__live-edge", pagesCss);
    const compact = cssBlock("@media (max-width: 979px)", pagesCss);

    expect(pagesCss).toMatch(
      /\.meeting-detail \{\s+--meeting-live:[^}]+align-content: start;/,
    );
    expect(pagesCss).toMatch(
      /\.meeting-detail__transcript \{\s+max-height: none;[^}]+padding: 0;[^}]+white-space: normal;/,
    );
    expect(workspace).toContain(
      "grid-template-columns: minmax(220px, 0.72fr) minmax(0, 1.8fr);",
    );
    expect(document).toContain("max-width: 76ch;");
    expect(liveEdge).toContain("border-left: 2px solid var(--meeting-live);");
    expect(cssRule(".meeting-detail__workspace", compact)).toContain(
      "grid-template-columns: minmax(0, 1fr);",
    );
  });

  it("keeps rendered report prose on the token-backed reading scale", () => {
    const root = cssRule(":root", tokensCss);

    for (const declaration of [
      "--prose-body: 13px;",
      "--prose-leading: 21px;",
      "--prose-h1: 19px;",
      "--prose-h2: 15px;",
      "--prose-measure: 76ch;",
    ]) {
      expect(root).toContain(declaration);
    }

    const doc = cssRule(".markdown-doc", componentsCss);
    expect(doc).toContain("font-size: var(--prose-body);");
    expect(doc).toContain("line-height: var(--prose-leading);");
    expect(doc).toContain("max-width: var(--prose-measure);");

    // Tailwind's preflight strips list markers, so prose must restore them.
    expect(componentsCss).toMatch(
      /\.markdown-doc ul \{[^}]*list-style-type: disc;/,
    );
    expect(componentsCss).toMatch(
      /\.markdown-doc ol \{[^}]*list-style-type: decimal;/,
    );

    // Wide tables scroll in their own box; the page itself is overflow-x: hidden.
    expect(cssRule(".markdown-doc__scroll", componentsCss)).toContain(
      "overflow-x: auto;",
    );
  });

  it("keeps seven transcript day cells shrinkable", () => {
    const week = cssRule(".transcript-pagination__week");

    expect(week).toContain("grid-template-columns: repeat(7, minmax(0, 1fr));");
    expect(cssRule(".transcript-pagination__day", pagesCss)).toContain(
      "min-width: 0;",
    );
  });

  it("separates transcript archive, browse, export, and selection layers", () => {
    expect(cssRule(".transcript-archive-rail", componentsCss)).toContain(
      "grid-template-columns: minmax(0, 1fr) auto;",
    );
    expect(cssRule(".transcript-browse-row", componentsCss)).toContain(
      "justify-content: space-between;",
    );
    expect(cssRule(".transcript-export-status", componentsCss)).toContain(
      "grid-template-columns: minmax(0, 1fr) auto;",
    );
    const selection = cssRule(".transcript-selection-shelf", componentsCss);
    expect(selection).toContain("position: sticky;");
    expect(selection).toContain("top: 0;");
    expect(selection).toContain("flex-wrap: wrap;");
  });

  it("stacks transcript workspace layers without horizontal overflow", () => {
    const compact = cssBlock("@media (max-width: 979px)", pagesCss);

    expect(cssRule(".transcript-archive-rail", compact)).toContain(
      "grid-template-columns: minmax(0, 1fr);",
    );
    expect(cssRule(".transcript-browse-row", compact)).toContain(
      "align-items: stretch;",
    );
    expect(cssRule(".transcript-browse-row__filters", compact)).toContain(
      "justify-content: flex-start;",
    );
  });

  it("keeps the seven-day picker legible at the narrowest supported width", () => {
    const narrow = cssBlock("@media (max-width: 640px)", componentsCss);

    expect(cssRule(".transcript-pagination", narrow)).toContain(
      "grid-template-columns: auto minmax(0, 1fr) auto;",
    );
    const week = cssRule(".transcript-pagination__week", narrow);
    expect(week).toContain(
      "grid-template-columns: repeat(7, minmax(48px, 1fr));",
    );
    expect(week).toContain("overflow-x: auto;");
  });

  it("uses theme-aware accessible transcript calendar colors", () => {
    const active = cssRule(
      '.transcript-pagination__day[aria-current="page"]',
      componentsCss,
    );
    const activeLabels = cssRule(
      '.transcript-pagination__day[aria-current="page"]\n  .transcript-pagination__day-label,\n.transcript-pagination__day[aria-current="page"]\n  .transcript-pagination__day-number',
      componentsCss,
    );

    expect(active).toContain("color: var(--primary-foreground);");
    expect(activeLabels).toContain("color: var(--primary-foreground);");
    expect(componentsCss).toContain("color: var(--calendar-sunday);");
    expect(componentsCss).toContain("color: var(--calendar-saturday);");
  });

  it("turns Activity rows into labeled cards and stacks Settings controls", () => {
    const compact = cssBlock("@media (max-width: 979px)", pagesCss);
    const narrow = cssBlock("@media (max-width: 820px)", pagesCss);

    expect(compact).toContain('[data-slot="queue-table"] tbody > tr');
    expect(compact).toContain("content: attr(data-label);");
    expect(compact).not.toContain(".settings-grid");
    expect(compact).toContain(".settings-inline-control");
    expect(compact).toContain("grid-template-columns: minmax(0, 1fr) auto;");
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
    expect(barRule).toContain(
      "transition: y 80ms linear, height 80ms linear, opacity 80ms linear;",
    );
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
    expect(rowRule).toContain("contain-intrinsic-size: auto var(--row-dense);");
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
    const lightRule = cssRule(
      ':root[data-theme="light"],\n[data-theme="light"]',
    );
    const pathPatternCodeRule = cssRule(".path-pattern code");
    const pathPatternLabelRule = cssRule(
      ".path-pattern span,\n.offline-note span",
    );

    expect(rootRule).toContain("--code-text: #e7edf5;");
    expect(rootRule).toContain("--code-bg: rgba(231, 237, 245, 0.08);");
    expect(lightRule).toContain("--code-text: #0f172a;");
    expect(css).toContain(
      "code {\n  overflow-wrap: anywhere;\n  color: var(--code-text);",
    );
    expect(pathPatternCodeRule).toContain("background: var(--code-bg);");
    expect(pathPatternCodeRule).toContain(
      "border: 1px solid var(--code-border);",
    );
    expect(pathPatternLabelRule).toContain("color: var(--text);");
  });
});

describe("transcript archive density", () => {
  it("keeps transcript rows compact for dense transcript lists", () => {
    const entryRule = cssRule(".transcript-entry");
    const dividerRule = cssRule(".transcript-entry:not(:last-child)");
    const actionsRule = cssRule(".transcript-entry__actions");
    const buttonRule = cssRule(
      ".transcript-entry__play,\n.transcript-entry__folder",
    );
    const textRule = cssRule(".transcript-entry__text");
    const checkboxRule = cssRule('.transcript-entry > [data-slot="checkbox"]');
    const sourceBadgeRule = cssRule(".transcript-source-badge");
    const selectedRule = cssRule('.transcript-entry[data-selected="true"]');

    expect(entryRule).toContain("display: grid;");
    expect(entryRule).toContain(
      "grid-template-columns: var(--size-selection-control) 116px 88px minmax(0, 1fr) 51px;",
    );
    expect(entryRule).toContain("align-items: center;");
    expect(entryRule).toContain("column-gap: 6px;");
    expect(entryRule).toContain("border-left: 2px solid transparent;");
    expect(entryRule).toContain("padding: 1px 6px 1px 4px;");
    expect(entryRule).toContain("border-radius: 0;");
    expect(checkboxRule).toContain("justify-self: center;");
    expect(sourceBadgeRule).toContain("max-width: 88px;");
    expect(dividerRule).toContain("border-bottom: 1px solid");
    expect(actionsRule).toContain("min-width: 51px;");
    expect(buttonRule).toContain("width: var(--space-6);");
    expect(buttonRule).toContain("height: var(--space-6);");
    expect(buttonRule).toContain("min-width: var(--space-6);");
    expect(selectedRule).toContain("border-left-color: var(--primary);");
    expect(selectedRule).not.toContain("box-shadow");
    expect(textRule).toContain("line-height: var(--leading-body);");
  });

  it("keeps audio merge progress compact and long save paths wrap-safe", () => {
    const statusRule = cssRule(".transcript-audio-merge-status");
    const copyRule = cssRule(".transcript-audio-merge-status__copy");
    const naturalTextRule = cssRule(
      ".transcript-audio-merge-status__copy strong,\n.transcript-audio-merge-status__copy span",
    );
    const pathRule = cssRule(".transcript-audio-merge-status__copy code");

    expect(statusRule).toContain("grid-template-columns: minmax(0, 1fr) auto;");
    expect(statusRule).toContain("align-items: center;");
    expect(copyRule).toContain("min-width: 0;");
    expect(naturalTextRule).toContain("word-break: keep-all;");
    expect(pathRule).toContain("overflow-wrap: anywhere;");
    expect(pathRule).toContain("word-break: normal;");
  });
});

describe("Activity summary severity layout", () => {
  it("auto-fits every semantic bucket and keeps warning and danger tokens", () => {
    const statsRule = cssRule(".queue-stats", componentsCss);
    expect(statsRule).toContain(
      "grid-template-columns: repeat(auto-fit, minmax(104px, 1fr));",
    );
    expect(componentsCss).toContain(
      '.queue-stats div[data-tone="warning"] strong',
    );
    expect(componentsCss).toContain(
      '.queue-stats div[data-tone="danger"] strong',
    );
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
      return [
        {
          property: declaration.slice(0, separator).trim(),
          value: declaration.slice(separator + 1).trim(),
        },
      ];
    });
}
