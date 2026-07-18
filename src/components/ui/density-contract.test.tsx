// @ts-ignore Vitest runs source assertions in Node; app tsconfig omits Node types.
import { readFileSync } from "node:fs";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Badge } from "./badge";
import { Button } from "./button";
import { Card, CardContent, CardHeader, CardTitle } from "./card";
import { FieldRow } from "./field-row";
import { Input } from "./input";
import { Switch } from "./switch";
import { Textarea } from "./textarea";
import { Toggle } from "./toggle";

const tokens = readFileSync(
  new URL("../../styles/tokens.css", import.meta.url),
  "utf8",
);
const densityPrimitiveSources = [
  "button.tsx",
  "tabs.tsx",
  "select.tsx",
  "toggle.tsx",
].map((file) =>
  readFileSync(new URL(file, import.meta.url), "utf8"),
).join("\n");
const switchSource = readFileSync(
  new URL("switch.tsx", import.meta.url),
  "utf8",
);

describe("shared UI density contract", () => {
  it("defines the shared selection and icon sizes", () => {
    expect(tokens).toContain("--size-selection-control: 16px");
    expect(tokens).toContain("--size-icon-sm: 14px");
    expect(tokens).toContain("--size-icon-md: 16px");
  });

  it("uses token-backed control heights and body typography", () => {
    const markup = renderToStaticMarkup(
      <>
        <Button>Default</Button>
        <Button size="sm">Compact</Button>
        <Button size="lg">Primary</Button>
        <Button size="xs">Extra compact alias</Button>
        <Badge>Ready</Badge>
        <Input aria-label="Name" />
        <Textarea aria-label="Notes" />
        <Toggle>Toggle</Toggle>
        <Switch />
      </>,
    );

    expect(markup).toContain("h-[var(--control-default)]");
    expect(markup).toContain("h-[var(--control-compact)]");
    expect(markup).toContain("h-[var(--control-primary)]");
    expect(markup).toContain("text-[length:var(--text-body)]");
    expect(markup).toContain("text-[length:var(--text-caption)]");
    expect(markup).not.toContain("transition-all");
    expect(densityPrimitiveSources).not.toMatch(
      /\b(?:text-xs|text-sm|h-6|h-7|h-8|h-9|size-6)\b/,
    );
    expect(switchSource).toContain("var(--switch-height)");
    expect(switchSource).not.toMatch(/\[(?:18\.4|32|14|24)px\]/);
  });

  it("uses the shared card and field rhythm", () => {
    const markup = renderToStaticMarkup(
      <>
        <Card>
          <CardHeader>
            <CardTitle>Recorder</CardTitle>
          </CardHeader>
          <CardContent>Content</CardContent>
        </Card>
        <FieldRow
          label="Model"
          description="Choose a model"
          control={<Input />}
        />
      </>,
    );

    expect(markup).toContain("rounded-[var(--radius-card)]");
    expect(markup).toContain("[--card-spacing:var(--space-4)]");
    expect(markup).toContain("gap-[var(--space-3)]");
    expect(markup).toContain("text-[length:var(--text-label)]");
  });
});
