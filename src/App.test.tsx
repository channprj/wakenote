import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import App from "./App";

describe("App branding", () => {
  it("does not show the old sidebar tagline", () => {
    const markup = renderToStaticMarkup(<App />);

    expect(markup).toContain("WakeNote");
    expect(markup).not.toContain("Voice-activated meeting notes");
  });
});
