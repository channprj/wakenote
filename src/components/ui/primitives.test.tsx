import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Slider } from "./primitives";

describe("Slider", () => {
  it("keeps the full field as a label when no help button is present", () => {
    const markup = renderToStaticMarkup(
      <Slider
        label="Overlay font size"
        value={24}
        min={18}
        max={48}
        suffix=" px"
        onValueChange={() => {}}
      />,
    );

    expect(markup).toMatch(/^<label class="ui-field">/);
    expect(markup).toContain("Overlay font size");
    expect(markup).toContain("24 px");
    expect(markup).toMatch(/<input[^>]*type="range"/);
    expect(markup).toMatch(/<\/label>$/);
  });

  it("uses an explicit label when a help button is present", () => {
    const markup = renderToStaticMarkup(
      <Slider
        label="Threshold"
        help="Threshold help"
        value={-40}
        min={-90}
        max={-10}
        suffix=" dBFS"
        onValueChange={() => {}}
      />,
    );

    const inputId = markup.match(/<input id="([^"]+)"/)?.[1];
    expect(markup).toMatch(/^<div class="ui-field">/);
    expect(inputId).toBeTruthy();
    expect(markup).toContain(`<label for="${inputId}">Threshold</label>`);
    expect(markup).toContain('aria-label="About Threshold"');
  });
});
