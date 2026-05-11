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

  it("omits the Queue status-card data-tone attribute when the queue is fully clean", () => {
    // Default mockSnapshot has every queue counter === 0 (pending/running/failed/completed),
    // so the Queue card should render without a data-tone attribute — the tone palette
    // (danger/primary/warning/success) only kicks in when at least one counter is non-zero,
    // mirroring QueuePanel queue-stats' per-stat tone signal.
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toMatch(
      /<span>Queue<\/span>\s*<strong>0 pending \xb7 0 running \xb7 0 failed \xb7 0 completed<\/strong>/,
    );
    expect(markup).not.toMatch(/<div [^>]*data-tone="(danger|primary|warning|success)"[^>]*>\s*<svg[^>]*lucide-list-todo/);
  });

  it("omits the Level status-card data-tone attribute when live input is inactive", () => {
    // Default mockSnapshot has live_input_active === false and current_dbfs === -120 (silence),
    // so the Level card should render without a data-tone attribute — the primary tone only
    // kicks in when capture is active AND current dBFS is at or above the trigger threshold.
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toMatch(
      /<span>Level<\/span>\s*<strong>-120 dBFS current \xb7 -120 dBFS peak<\/strong>/,
    );
    expect(markup).not.toMatch(/<div [^>]*data-tone="primary"[^>]*>\s*<svg[^>]*lucide-activity/);
  });
});
