import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type Props = {
  text: string;
  streaming?: boolean;
  onOpenFile?: (path: string) => void;
};

export default function AssistantMessage({ text, streaming, onOpenFile }: Props) {
  return (
    <div className="web-msg-assistant">
      <div className="web-msg-assistant-body">
        <ReactMarkdown remarkPlugins={[remarkGfm]} components={{
          a: ({ href, children, ...props }) => {
            const external = Boolean(href && /^(?:https?:|mailto:)/i.test(href));
            const navigational = external || Boolean(href?.startsWith("#"));
            return <a
              href={href}
              {...props}
              className={external ? "web-external-link" : undefined}
              target={external ? "_blank" : undefined}
              rel={external ? "noopener noreferrer" : undefined}
              onClick={(event) => {
              if (!navigational && href && onOpenFile) {
                event.preventDefault();
                const decoded = decodeURIComponent(href);
                const path = decoded.startsWith("file://") ? new URL(decoded).pathname : decoded;
                onOpenFile(path.replace(/^\.\//, ""));
              }
              }}
            >{children}</a>;
          },
        }}>{text}</ReactMarkdown>
        {streaming && <span className="web-streaming-cursor" />}
      </div>
    </div>
  );
}
