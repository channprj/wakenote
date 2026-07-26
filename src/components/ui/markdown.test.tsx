import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { LONG_CONTENT } from "@/test-fixtures/long-content";
import { MarkdownDocument } from "./markdown";

function render(content: string) {
  return renderToStaticMarkup(<MarkdownDocument content={content} />);
}

describe("MarkdownDocument", () => {
  it("renders headings as a hierarchy instead of literal hashes", () => {
    const markup = render("# Summary\n\n## Key points\n\n### Detail");

    expect(markup).toContain("<h1>Summary</h1>");
    expect(markup).toContain("<h2>Key points</h2>");
    expect(markup).toContain("<h3>Detail</h3>");
    expect(markup).not.toContain("# Summary");
  });

  it("renders emphasis, lists, and blockquotes", () => {
    const markup = render(
      [
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
    );

    expect(markup).toContain("<strong>Decided</strong>");
    expect(markup).toContain("<li>first point</li>");
    expect(markup).toContain("<ol>");
    expect(markup).toContain("<blockquote>");
    expect(markup).not.toContain("**Decided**");
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

  it("renders GFM task lists as checkboxes", () => {
    const markup = render("- [x] shipped\n- [ ] pending");

    expect(markup).toContain('type="checkbox"');
    expect(markup).toContain("checked");
    expect(markup).not.toContain("[x]");
  });

  it("renders fenced code blocks without collapsing whitespace", () => {
    const markup = render("```text\nsummary   4,800 tokens\n```");

    expect(markup).toContain("<pre>");
    expect(markup).toContain("<code");
    expect(markup).toContain("summary   4,800 tokens");
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
