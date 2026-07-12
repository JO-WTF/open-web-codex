import type { MessageEntry } from "./MessageList";
import type { GoalInfo } from "./GoalBanner";
import ThinkingIndicator from "./ThinkingIndicator";
import Header from "./Header";
import MessageList from "./MessageList";
import Composer from "./Composer";
import GoalBanner from "./GoalBanner";

type Props = {
  goal: GoalInfo | null;
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
  onResolveApproval?: (workspaceId: string, threadId: string, decision: string) => void;
};

export default function Conversation({
  goal,
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
  onResolveApproval,
}: Props) {
  return (
    <section className="web-chat">
      <Header workspaceName={workspaceName} threadTitle={threadTitle} tokenUsage={tokenUsage} threadStatus={threadStatus} threadSettings={threadSettings} />
      <div className="web-message-area">
        {thinking && <ThinkingIndicator />}
        <MessageList items={messages} onResolveApproval={onResolveApproval} />
      </div>
      {goal && (
        <GoalBanner goal={goal} />
      )}
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
