import { FileTextIcon } from "lucide-react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Button } from "./button";
import { EmptyState } from "./empty-state";

describe("EmptyState", () => {
  it("names what is missing and explains it", () => {
    const markup = renderToStaticMarkup(
      <EmptyState
        icon={FileTextIcon}
        title="No reports yet"
        description="Reports are written from your captured transcripts."
      />,
    );

    expect(markup).toContain('data-slot="empty-state"');
    expect(markup).toContain("No reports yet");
    expect(markup).toContain(
      "Reports are written from your captured transcripts.",
    );
    expect(markup).toContain('data-slot="empty-icon"');
  });

  it("offers the action that resolves the empty state", () => {
    const markup = renderToStaticMarkup(
      <EmptyState
        title="No reports yet"
        action={<Button type="button">Choose transcripts</Button>}
      />,
    );

    expect(markup).toContain('data-slot="empty-content"');
    expect(markup).toContain("Choose transcripts");
  });

  it("omits the description and action slots when not supplied", () => {
    const markup = renderToStaticMarkup(<EmptyState title="Loading reports" />);

    expect(markup).toContain("Loading reports");
    expect(markup).not.toContain('data-slot="empty-description"');
    expect(markup).not.toContain('data-slot="empty-content"');
  });

  it("supports a spinning icon for loading placeholders", () => {
    const markup = renderToStaticMarkup(
      <EmptyState
        icon={FileTextIcon}
        iconClassName="loading-spin"
        title="Loading reports"
      />,
    );

    expect(markup).toContain("loading-spin");
  });

  it("keeps the shared surface class alongside a caller's modifier", () => {
    const markup = renderToStaticMarkup(
      <EmptyState className="transcripts-empty" title="No transcripts" />,
    );

    expect(markup).toContain("empty-state");
    expect(markup).toContain("transcripts-empty");
  });
});
