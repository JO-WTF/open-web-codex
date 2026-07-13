import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { MessageEntry } from "./MessageList";
import type { GoalInfo } from "./GoalBanner";
import ThinkingIndicator from "./ThinkingIndicator";
import Header from "./Header";
import MessageList from "./MessageList";
import Composer from "./Composer";
import GoalBanner from "./GoalBanner";
import {
  initialConversationStart,
  previousConversationStart,
} from "./conversationWindow";

type Props = {
  goal: GoalInfo | null;
  workspaceName: string | null;
  threadTitle: string | null;
  conversationId: string | null;
  tokenUsage: import("../../types").ThreadTokenUsage | null;
  threadStatus: string;
  threadSettings: Record<string, unknown> | null;
  messages: MessageEntry[];
  workspaceId?: string;
  thinking?: boolean;
  draft: string;
  onDraftChange: (text: string) => void;
  onSend: () => void;
  busy: boolean;
  sendDisabled: boolean;
  onResolveApproval?: (workspaceId: string, requestId: number | string, decision: "accept" | "decline") => void;
};

export default function Conversation({
  goal,
  workspaceName,
  threadTitle,
  conversationId,
  tokenUsage,
  threadStatus,
  threadSettings,
  messages,
  workspaceId,
  thinking,
  draft,
  onDraftChange,
  onSend,
  busy,
  sendDisabled,
  onResolveApproval,
}: Props) {
  const messageAreaRef = useRef<HTMLDivElement | null>(null);
  const isAtBottomRef = useRef(true);
  const pendingInitialWindowRef = useRef(true);
  const [visibleStart, setVisibleStart] = useState(() => initialConversationStart(messages));
  const visibleMessages = useMemo(() => messages.slice(visibleStart), [messages, visibleStart]);
  const hasOlderMessages = visibleStart > 0;

  useEffect(() => {
    isAtBottomRef.current = true;
    pendingInitialWindowRef.current = true;
    setVisibleStart(initialConversationStart(messages));
  }, [conversationId]);

  useEffect(() => {
    if (!pendingInitialWindowRef.current || messages.length === 0) return;
    pendingInitialWindowRef.current = false;
    setVisibleStart(initialConversationStart(messages));
  }, [messages]);

  useLayoutEffect(() => {
    const area = messageAreaRef.current;
    if (!area || !isAtBottomRef.current) return;
    area.scrollTop = area.scrollHeight;
  }, [conversationId, messages.length, visibleStart]);

  const loadOlderMessages = useCallback(() => {
    setVisibleStart((current) => previousConversationStart(messages, current));
  }, [messages]);

  const handleScroll = useCallback(() => {
    const area = messageAreaRef.current;
    if (!area) return;
    const distanceFromBottom = area.scrollHeight - area.scrollTop - area.clientHeight;
    isAtBottomRef.current = distanceFromBottom <= 24;
    if (area.scrollTop <= 24 && visibleStart > 0) {
      loadOlderMessages();
    }
  }, [loadOlderMessages, visibleStart]);

  return (
    <section className="web-chat">
      <Header workspaceName={workspaceName} threadTitle={threadTitle} tokenUsage={tokenUsage} threadStatus={threadStatus} threadSettings={threadSettings} />
      <div className="web-message-area" ref={messageAreaRef} onScroll={handleScroll}>
        {hasOlderMessages && (
          <button type="button" className="web-load-older" onClick={loadOlderMessages}>
            Load previous messages
          </button>
        )}
        {thinking && <ThinkingIndicator />}
        <MessageList
          items={visibleMessages}
          workspaceId={workspaceId}
          onResolveApproval={onResolveApproval}
        />
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
