import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { StatusBadge } from "./status-badge";

describe("StatusBadge", () => {
  it("maps every tone to a stable semantic badge", () => {
    const tones = [
      "neutral",
      "primary",
      "success",
      "warning",
      "danger",
    ] as const;

    for (const tone of tones) {
      const markup = renderToStaticMarkup(<StatusBadge tone={tone}>State</StatusBadge>);

      expect(markup, tone).toContain(`data-tone="${tone}"`);
      expect(markup, tone).toContain('data-slot="badge"');
      expect(markup, tone).toContain("State");
    }
  });
});
