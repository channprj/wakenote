import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AppFrame } from "./AppFrame";
import { PageHeader } from "./PageHeader";

describe("AppFrame", () => {
  it("keeps page content and the optional recording rail in separate rows", () => {
    const markup = renderToStaticMarkup(
      <AppFrame
        activeRoute="transcripts"
        queueAttentionCount={0}
        queueAttentionTone="warning"
        onNavigate={() => {}}
        statusRail={<div data-testid="rail">Recording</div>}
      >
        <p>Archive</p>
      </AppFrame>,
    );

    expect(markup).toContain('class="app-frame"');
    expect(markup).toContain('class="app-viewport"');
    expect(markup).toContain('class="app-page"');
    expect(markup).toMatch(/class="app-page">[\s\S]*?Archive[\s\S]*?<\/div><div data-testid="rail"/);
  });
});

describe("PageHeader", () => {
  it("wraps long descriptions separately from fixed actions", () => {
    const markup = renderToStaticMarkup(
      <PageHeader
        eyebrow="Workspace"
        title="Transcripts"
        description="Review every short capture without clipping the available actions."
        actions={<button type="button">Refresh</button>}
      />,
    );

    expect(markup).toContain('data-slot="page-header"');
    expect(markup).toContain('data-slot="page-header-copy"');
    expect(markup).toContain('data-slot="page-header-actions"');
    expect(markup).toContain("overflow-wrap-anywhere");
  });
});
