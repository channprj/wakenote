import type { ComponentProps } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, expectTypeOf, it } from "vitest";
import type { StatusTone } from "@/lib/status-summary";
import { StatusBadge } from "./status-badge";

describe("StatusBadge", () => {
  it.each(["neutral", "primary", "success", "warning", "danger"] as const)(
    "maps %s to a stable semantic tone",
    (tone) => {
      const markup = renderToStaticMarkup(<StatusBadge tone={tone}>State</StatusBadge>);

      expect(markup).toContain(`data-tone="${tone}"`);
      expect(markup).toContain('data-slot="badge"');
      expect(markup).toContain("State");
    },
  );

  it("uses the canonical StatusTone type", () => {
    type Tone = ComponentProps<typeof StatusBadge>["tone"];
    expectTypeOf<Tone>().toEqualTypeOf<StatusTone>();
  });
});
