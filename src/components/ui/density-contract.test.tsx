import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Badge } from "./badge";
import { Button } from "./button";
import { Card, CardContent, CardHeader, CardTitle } from "./card";
import { FieldRow } from "./field-row";
import { Input } from "./input";
import { Textarea } from "./textarea";

describe("shared UI density contract", () => {
  it("uses token-backed control heights and body typography", () => {
    const markup = renderToStaticMarkup(
      <>
        <Button>Default</Button>
        <Button size="sm">Compact</Button>
        <Button size="lg">Primary</Button>
        <Badge>Ready</Badge>
        <Input aria-label="Name" />
        <Textarea aria-label="Notes" />
      </>,
    );

    expect(markup).toContain("h-[var(--control-default)]");
    expect(markup).toContain("h-[var(--control-compact)]");
    expect(markup).toContain("h-[var(--control-primary)]");
    expect(markup).toContain("text-[length:var(--text-body)]");
    expect(markup).toContain("text-[length:var(--text-caption)]");
    expect(markup).not.toContain("transition-all");
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
