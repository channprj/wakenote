import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App branding", () => {
  it("does not show the old sidebar tagline", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toContain("WakeNote");
    expect(markup).not.toContain("Voice-activated meeting notes");
  });

  it("does not render the About menu item", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).not.toContain("About");
  });

  it("title-cases the status-hero tray_state Badge text", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toMatch(/<div class="status-strip">[\s\S]*?<span class="ui-badge[^"]*">Idle<\/span>/);
    expect(markup).not.toMatch(/<span class="ui-badge[^"]*">idle<\/span>/);
  });

  it("omits the Queue status-card data-tone attribute when no failed jobs exist", () => {
    // Default mockSnapshot has queue.failed_count === 0, so the Queue card should render
    // without a data-tone attribute (the danger-toned strong color is only emitted when
    // failed_count > 0, mirroring QueuePanel queue-stats' per-stat tone signal).
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toMatch(/<span>Queue<\/span>\s*<strong>0 pending \xb7 0 running \xb7 0 failed<\/strong>/);
    expect(markup).not.toMatch(/<div [^>]*data-tone="danger"[^>]*>\s*<svg[^>]*lucide-list-todo/);
  });
});
