import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import MapReplyCard from "./MapReplyCard";
import { parseReplyCards } from "../../../utils/replyCards";

type Props = {
  text: string;
  streaming?: boolean;
  onOpenFile?: (path: string) => void;
};

function MarkdownText({ text, onOpenFile }: { text: string; onOpenFile?: Props["onOpenFile"] }) {
  return (
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
  );
}

export default function AssistantMessage({ text, streaming, onOpenFile }: Props) {
  const parts = parseReplyCards(text);
  return (
    <div className="web-msg-assistant">
      <div className="web-msg-assistant-body">
        {parts.map((part, index) => part.type === "text" ? (
          <MarkdownText key={`text-${index}`} text={part.content} onOpenFile={onOpenFile} />
        ) : (
          <MapReplyCard key={part.id || `map-${index}`} card={part} />
        ))}
        {streaming && <span className="web-streaming-cursor" />}
      </div>
    </div>
  );
}
