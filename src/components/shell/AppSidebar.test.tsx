import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AppSidebar } from "./AppSidebar";

describe("AppSidebar", () => {
  it("renders separate compact destinations with Settings pinned apart", () => {
    const markup = renderToStaticMarkup(
      <AppSidebar
        activeRoute="meetings"
        queueAttentionCount={2}
        onNavigate={() => {}}
      />,
    );

    expect(markup).toContain('aria-label="Primary navigation"');
    expect(markup).toMatch(/data-route="meetings"[^>]*data-active="true"/);
    expect(markup).toContain(">Meetings</span>");
    expect(markup).toContain(">Transcripts</span>");
    expect(markup).not.toContain(">Library<");
    expect(markup).toMatch(/data-route="settings"/);
    expect(markup).toContain(`v${__APP_VERSION__}`);
  });

  it("shows queue attention only beside Activity", () => {
    const markup = renderToStaticMarkup(
      <AppSidebar
        activeRoute="capture"
        queueAttentionCount={3}
        onNavigate={() => {}}
      />,
    );

    expect(markup).toMatch(
      /data-route="activity"[\s\S]*?data-tone="danger"[\s\S]*?>3<\/span>/,
    );
    expect(markup.match(/data-tone="danger"/g)).toHaveLength(1);
  });

  it("keeps every icon-rail destination named and caps large attention counts", () => {
    const markup = renderToStaticMarkup(
      <AppSidebar
        activeRoute="capture"
        queueAttentionCount={120}
        onNavigate={() => {}}
      />,
    );

    for (const label of [
      "Capture",
      "Meetings",
      "Transcripts",
      "Reports",
      "Activity",
      "Settings",
    ]) {
      expect(markup).toContain(`aria-label="${label}"`);
    }
    expect(markup).toContain(">99+</span>");
    expect(markup).not.toContain(">120</span>");
  });
});
