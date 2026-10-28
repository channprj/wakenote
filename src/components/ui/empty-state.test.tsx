import { FileTextIcon } from "lucide-react";
import type { ReactElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Button } from "./button";
import { EmptyState } from "./empty-state";

describe("EmptyState", () => {
  it("renders each optional slot only when supplied", () => {
    const cases: Array<{
      label: string;
      element: ReactElement;
      present: string[];
      absent: string[];
    }> = [
      {
        label: "icon and description name what is missing",
        element: (
          <EmptyState
            icon={FileTextIcon}
            title="No reports yet"
            description="Reports are written from your captured transcripts."
          />
        ),
        present: [
          'data-slot="empty-state"',
          "No reports yet",
          "Reports are written from your captured transcripts.",
          'data-slot="empty-icon"',
        ],
        absent: [],
      },
      {
        label: "action resolves the empty state",
        element: (
          <EmptyState
            title="No reports yet"
            action={<Button type="button">Choose transcripts</Button>}
          />
        ),
        present: ['data-slot="empty-content"', "Choose transcripts"],
        absent: [],
      },
      {
        label: "description and action omitted",
        element: <EmptyState title="Loading reports" />,
        present: ["Loading reports"],
        absent: [
          'data-slot="empty-description"',
          'data-slot="empty-content"',
        ],
      },
    ];

    for (const { label, element, present, absent } of cases) {
      const markup = renderToStaticMarkup(element);

      for (const fragment of present) {
        expect(markup, label).toContain(fragment);
      }
      for (const fragment of absent) {
        expect(markup, label).not.toContain(fragment);
      }
    }
  });

  it("passes caller classes to the icon and surface", () => {
    const markup = renderToStaticMarkup(
      <EmptyState
        icon={FileTextIcon}
        iconClassName="loading-spin"
        className="transcripts-empty"
        title="Loading reports"
      />,
    );

    expect(markup).toContain("loading-spin");
    expect(markup).toMatch(
      /class="[^"]*\bempty-state\b[^"]*\btranscripts-empty\b/,
    );
  });
});
