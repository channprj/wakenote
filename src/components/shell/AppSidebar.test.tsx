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
      /aria-label="Primary navigation"[\s\S]*data-route="activity"[\s\S]*data-route="webhooks"[\s\S]*data-route="settings"[\s\S]*<\/nav>/,
    );
    expect(markup).toContain(`v${__APP_VERSION__}`);
  });

  it("shows the queue attention badge beside Activity with its tone", () => {
    const cases = [
      ["danger", 3, 1],
      ["warning", 1, 0],
    ] as const;

    for (const [tone, count, dangerCount] of cases) {
      const markup = renderToStaticMarkup(
        <AppSidebar
          activeRoute="capture"
          queueAttentionCount={count}
          queueAttentionTone={tone}
          onNavigate={() => {}}
        />,
      );

      expect(markup, tone).toMatch(
        new RegExp(
          `data-route="activity"[\\s\\S]*?data-tone="${tone}"[\\s\\S]*?>${count}<\\/span>`,
        ),
      );
      expect(
        (markup.match(/data-tone="danger"/g) ?? []).length,
        tone,
      ).toBe(dangerCount);
    }
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
      "Webhooks",
      "Settings",
    ]) {
      expect(markup).toContain(`aria-label="${label}"`);
    }
    expect(markup).toContain(">99+</span>");
    expect(markup).not.toContain(">120</span>");
  });
});
