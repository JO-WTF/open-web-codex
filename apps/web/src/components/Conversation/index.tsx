import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { MessageEntry } from "./MessageList";
import type { GoalInfo } from "./GoalBanner";
import ThinkingIndicator from "./ThinkingIndicator";
import Header from "./Header";
import MessageList from "./MessageList";
import Composer from "./Composer";
import GoalBanner from "./GoalBanner";
import FollowUpQueue, { type QueuedFollowUp } from "./FollowUpQueue";
import UserInputCard from "./messages/UserInputCard";
import type { RequestUserInputRequest, RequestUserInputResponse } from "../../types";
import {
  initialConversationStart,
  previousConversationStart,
} from "./conversationWindow";

type Props = {
  goal: GoalInfo | null;
  workspaceName: string | null;
  threadTitle: string | null;
  conversationId: string | null;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
  filePanelOpen?: boolean;
  onToggleFilePanel?: () => void;
  onOpenFile?: (path: string) => void;
  tokenUsage: import("../../types").ThreadTokenUsage | null;
  threadStatus: string;
  threadSettings: Record<string, unknown> | null;
  messages: MessageEntry[];
  workspaceId?: string;
  thinking?: boolean;
  draft: string;
  onDraftChange: (text: string) => void;
  onSend: () => void;
  onStop: () => void;
  stopping: boolean;
  queuedFollowUps: QueuedFollowUp[];
  steeringFollowUpId: string | null;
  canSteer: boolean;
  onSteerFollowUp: (id: string) => void;
  onDeleteFollowUp: (id: string) => void;
  userInputRequest: RequestUserInputRequest | null;
  submittingUserInput: boolean;
  onSubmitUserInput: (request: RequestUserInputRequest, response: RequestUserInputResponse) => void;
  busy: boolean;
  sendDisabled: boolean;
  onResolveApproval?: (workspaceId: string, requestId: number | string, decision: "accept" | "decline") => void;
};

export default function Conversation({
  goal,
  workspaceName,
  threadTitle,
  conversationId,
  sidebarCollapsed,
  onToggleSidebar,
  filePanelOpen = false,
  onToggleFilePanel,
  onOpenFile,
  tokenUsage,
  threadStatus,
  threadSettings,
  messages,
  workspaceId,
  thinking,
  draft,
  onDraftChange,
  onSend,
  onStop,
  stopping,
  queuedFollowUps,
  steeringFollowUpId,
  canSteer,
  onSteerFollowUp,
  onDeleteFollowUp,
  userInputRequest,
  submittingUserInput,
  onSubmitUserInput,
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
  }, [conversationId, messages, visibleStart]);

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
      <Header workspaceName={workspaceName} threadTitle={threadTitle} threadStatus={threadStatus} threadSettings={threadSettings} sidebarCollapsed={sidebarCollapsed} onToggleSidebar={onToggleSidebar} filePanelOpen={filePanelOpen} onToggleFilePanel={onToggleFilePanel} />
      <div className="web-message-area" ref={messageAreaRef} onScroll={handleScroll}>
        {hasOlderMessages && (
          <button type="button" className="web-load-older" onClick={loadOlderMessages}>
            Load previous messages
          </button>
        )}
        <MessageList
          items={visibleMessages}
          thinking={thinking}
          onOpenFile={onOpenFile}
          workspaceId={workspaceId}
          onResolveApproval={onResolveApproval}
        />
        {userInputRequest ? <UserInputCard request={userInputRequest} submitting={submittingUserInput} onSubmit={onSubmitUserInput} /> : null}
        {thinking && !visibleMessages.some((entry) => entry.level === "user") && <ThinkingIndicator />}
      </div>
      <GoalBanner goal={goal} />
      <FollowUpQueue
        items={queuedFollowUps}
        canSteer={canSteer}
        steeringId={steeringFollowUpId}
        onSteer={onSteerFollowUp}
        onDelete={onDeleteFollowUp}
      />
      <Composer
        draft={draft}
        onDraftChange={onDraftChange}
        onSend={onSend}
        onStop={onStop}
        running={thinking || threadStatus === "running" || threadStatus === "reconnecting" || threadStatus.startsWith("active")}
        stopping={stopping}
        busy={busy}
        disabled={sendDisabled}
        tokenUsage={tokenUsage}
      />
    </section>
  );
}
