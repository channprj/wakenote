import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AppSidebar } from "./AppSidebar";

describe("AppSidebar", () => {
  it("renders Settings as the final item in primary navigation", () => {
    const markup = renderToStaticMarkup(
      <AppSidebar
        activeRoute="meetings"
        queueAttentionCount={2}
        queueAttentionTone="danger"
        onNavigate={() => {}}
      />,
    );

    expect(markup).toContain('aria-label="Primary navigation"');
    expect(markup).toMatch(/data-route="meetings"[^>]*data-active="true"/);
    expect(markup).toContain(">Meetings</span>");
    expect(markup).toContain(">Transcripts</span>");
    expect(markup).not.toContain(">Library<");
    expect(markup).toMatch(
      /aria-label="Primary navigation"[\s\S]*data-route="activity"[\s\S]*data-route="settings"[\s\S]*<\/nav>/,
    );
    expect(markup).toContain(`v${__APP_VERSION__}`);
  });

  it("shows queue attention only beside Activity", () => {
    const markup = renderToStaticMarkup(
      <AppSidebar
        activeRoute="capture"
        queueAttentionCount={3}
        queueAttentionTone="danger"
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
        queueAttentionTone="danger"
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

  it("shows recent quality attention as a warning badge", () => {
    const markup = renderToStaticMarkup(
      <AppSidebar
        activeRoute="capture"
        queueAttentionCount={1}
        queueAttentionTone="warning"
        onNavigate={() => {}}
      />,
    );

    expect(markup).toMatch(
      /data-route="activity"[\s\S]*?data-tone="warning"[\s\S]*?>1<\/span>/,
    );
    expect(markup).not.toContain('data-tone="danger"');
  });
});
