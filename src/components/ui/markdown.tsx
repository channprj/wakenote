import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { cn } from "@/lib/utils";

/**
 * Renders a generated report body as a document.
 *
 * Reports come back from the LLM as GitHub-flavored Markdown — headings, tables,
 * task lists, fenced code, blockquotes. Rendering is deliberately the only thing
 * this component does, so `react-markdown` has exactly one call site.
 *
 * Raw HTML is NOT enabled (no `rehype-raw`): report bodies are model output and
 * are treated as untrusted, so `react-markdown`'s default HTML-escaping stays on.
 */
export function MarkdownDocument({
  content,
  className,
  ...props
}: { content: string } & Omit<React.ComponentProps<"div">, "children">) {
  return (
    <div
      data-slot="markdown-document"
      className={cn("markdown-doc", className)}
      {...props}
    >
      <Markdown
        remarkPlugins={[remarkGfm]}
        components={{
          // Generated images can encode private transcript text in their URL.
          // Keep their description without making an automatic network request.
          img: ({ alt }) => <span>{alt}</span>,
          // Wide tables scroll inside their own container: the page itself is
          // `overflow-x: hidden`, so an unwrapped table would be clipped.
          table: ({ node: _node, ...tableProps }) => (
            <div className="markdown-doc__scroll">
              <table {...tableProps} />
            </div>
          ),
          // Never navigate the app window away from the report.
          a: ({ node: _node, ...anchorProps }) => (
            <a {...anchorProps} target="_blank" rel="noreferrer noopener" />
          ),
        }}
      >
        {content}
      </Markdown>
    </div>
  );
}
