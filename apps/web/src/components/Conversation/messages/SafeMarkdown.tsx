import { useMemo } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";

type Props = {
  text: string;
  onOpenFile?: (path: string) => void;
};

export default function SafeMarkdown({ text, onOpenFile }: Props) {
  const components = useMemo<Components>(() => ({
    a: ({ href, children, node: _node, ...props }) => {
      const external = Boolean(href && /^(?:https?:|mailto:)/i.test(href));
      const navigational = external || Boolean(href?.startsWith("#"));
      return (
        <a
          href={href}
          {...props}
          className={external ? "web-external-link" : undefined}
          target={external ? "_blank" : undefined}
          rel={external ? "noopener noreferrer" : undefined}
          onClick={(event) => {
            if (!navigational && href && onOpenFile) {
              event.preventDefault();
              const decoded = decodeURIComponent(href);
              const path = decoded.startsWith("file://")
                ? new URL(decoded).pathname
                : decoded;
              onOpenFile(path.replace(/^\.\//, ""));
            }
          }}
        >
          {children}
        </a>
      );
    },
  }), [onOpenFile]);

  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={components}
      skipHtml
    >
      {text}
    </ReactMarkdown>
  );
}
