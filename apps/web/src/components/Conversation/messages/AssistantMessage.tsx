import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

type Props = {
  text: string;
  streaming?: boolean;
};

export default function AssistantMessage({ text, streaming }: Props) {
  return (
    <div className="web-msg-assistant">
      <div className="web-msg-assistant-body">
        <ReactMarkdown remarkPlugins={[remarkGfm]}>{text}</ReactMarkdown>
        {streaming && <span className="web-streaming-cursor" />}
      </div>
    </div>
  );
}
