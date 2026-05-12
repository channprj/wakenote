import { describe, expectTypeOf, it } from "vitest";
import type { ComponentProps } from "react";
import { Badge } from "./primitives";
import type { StatusTone } from "../../lib/status-summary";

// Pins the type-level invariant that Badge's `tone?` prop accepts exactly the canonical
// StatusTone union (plus undefined for the optional default). Mirrors the iter-68/72/73/75
// runtime-helper-family consolidation at the type level: the *BadgeTone helpers all return
// StatusTone, so Badge's prop type must structurally agree — otherwise adding a new
// StatusTone variant (e.g. "info") would require a parallel edit on the primitives.tsx
// literal union, which is exactly the duplication this invariant prevents.

describe("Badge tone prop type", () => {
  it("structurally matches StatusTone | undefined", () => {
    type BadgeTone = ComponentProps<typeof Badge>["tone"];

    expectTypeOf<BadgeTone>().toEqualTypeOf<StatusTone | undefined>();
  });

  it("accepts every StatusTone literal", () => {
    expectTypeOf<"neutral">().toMatchTypeOf<ComponentProps<typeof Badge>["tone"]>();
    expectTypeOf<"success">().toMatchTypeOf<ComponentProps<typeof Badge>["tone"]>();
    expectTypeOf<"warning">().toMatchTypeOf<ComponentProps<typeof Badge>["tone"]>();
    expectTypeOf<"danger">().toMatchTypeOf<ComponentProps<typeof Badge>["tone"]>();
    expectTypeOf<"primary">().toMatchTypeOf<ComponentProps<typeof Badge>["tone"]>();
  });

  it("rejects strings outside the StatusTone union", () => {
    type BadgeTone = ComponentProps<typeof Badge>["tone"];

    // @ts-expect-error — "info" is not part of the canonical StatusTone union.
    const _info: BadgeTone = "info";
    // @ts-expect-error — arbitrary strings are not assignable to the tone prop.
    const _arbitrary: BadgeTone = "anything";
    void _info;
    void _arbitrary;
  });
});
