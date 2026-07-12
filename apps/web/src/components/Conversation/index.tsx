import type { MessageEntry } from "./MessageList";
import ThinkingIndicator from "./ThinkingIndicator";
import Header from "./Header";
import MessageList from "./MessageList";
import Composer from "./Composer";

type Props = {
  workspaceName: string | null;
  threadTitle: string | null;
  tokenUsage: import("../../types").ThreadTokenUsage | null;
  threadStatus: string;
  threadSettings: Record<string, unknown> | null;
  messages: MessageEntry[];
  thinking?: boolean;
  draft: string;
  onDraftChange: (text: string) => void;
  onSend: () => void;
  busy: boolean;
  sendDisabled: boolean;
};

export default function Conversation({
  workspaceName,
  threadTitle,
  tokenUsage,
  threadStatus,
  threadSettings,
  messages,
  thinking,
  draft,
  onDraftChange,
  onSend,
  busy,
  sendDisabled,
}: Props) {
  return (
    <section className="web-chat">
      <Header workspaceName={workspaceName} threadTitle={threadTitle} tokenUsage={tokenUsage} threadStatus={threadStatus} threadSettings={threadSettings} />
      <div className="web-message-area">
        {thinking && <ThinkingIndicator />}
        <MessageList items={messages} />
      </div>
      <Composer
        draft={draft}
        onDraftChange={onDraftChange}
        onSend={onSend}
        busy={busy}
        disabled={sendDisabled}
      />
    </section>
  );
}
