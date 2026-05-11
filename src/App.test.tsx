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
});
