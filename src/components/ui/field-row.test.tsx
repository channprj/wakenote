import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { FieldRow } from "./field-row";

describe("FieldRow", () => {
  it("composes shadcn field slots into a compact horizontal row", () => {
    const markup = renderToStaticMarkup(
      <FieldRow
        label="Microphone"
        description="A very long device name remains inside the available row width."
        help={<button type="button">Help</button>}
        control={<button type="button">Select</button>}
      />,
    );

    expect(markup).toContain('data-slot="field-row"');
    expect(markup).toContain('data-slot="field"');
    expect(markup).toContain('data-slot="field-content"');
    expect(markup).toContain('data-slot="field-description"');
    expect(markup).toContain("min-w-0");
    expect(markup).toContain("overflow-wrap-anywhere");
    expect(markup).toContain("Microphone");
    expect(markup).toContain("Help");
    expect(markup).toContain("Select");
  });
});
