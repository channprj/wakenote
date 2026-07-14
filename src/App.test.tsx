import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App information architecture", () => {
  it("renders the compact application frame in dark theme by default", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toContain('class="app-frame"');
    expect(markup).toContain('data-theme="dark"');
    expect(markup).toContain('data-slot="capture-page"');
  });

  it("separates Meetings and Transcripts in primary navigation", () => {
    const markup = renderToStaticMarkup(<App />);

    for (const route of [
      "capture",
      "meetings",
      "transcripts",
      "reports",
      "activity",
      "settings",
    ]) {
      expect(markup).toContain(`data-route="${route}"`);
    }
    expect(markup).toContain(">Meetings</span>");
    expect(markup).toContain(">Transcripts</span>");
    expect(markup).not.toContain("Library");
  });

  it("removes the legacy status dashboard", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).not.toContain('class="status-hero"');
    expect(markup).not.toContain('class="status-cards"');
  });

  it("keeps version information in the sidebar utility area", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toContain(`v${__APP_VERSION__}`);
  });
});
