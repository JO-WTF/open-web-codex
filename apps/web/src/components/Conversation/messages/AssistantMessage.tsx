type Props = {
  text: string;
  streaming?: boolean;
};

export default function AssistantMessage({ text, streaming }: Props) {
  return (
    <div className="web-msg-assistant">
      <div className="web-msg-assistant-body">
        {text}
        {streaming && <span className="web-streaming-cursor" />}
      </div>
    </div>
  );
}
