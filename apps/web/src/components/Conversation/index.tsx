import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { MessageEntry } from "./MessageList";
import type { GoalInfo } from "./GoalBanner";
import ThinkingIndicator from "./ThinkingIndicator";
import Header from "./Header";
import MessageList from "./MessageList";
import Composer from "./Composer";
import GoalBanner from "./GoalBanner";
import FollowUpQueue, { type QueuedFollowUp } from "./FollowUpQueue";
import UserInputQueue from "./UserInputQueue";
import McpFormQueue from "./McpFormQueue";
import type {
  ArtifactSummary,
  McpFormContent,
  McpFormResponseAction,
  PendingMcpFormSummary,
  PendingUserInputSummary,
} from "../../../browser/types";
import type { ModelProviderSummary, ModelSummary } from "./Composer";
import TaskApprovalQueue, {
  type TaskApprovalRequest,
} from "./TaskApprovalQueue";
import {
  initialConversationStart,
  previousConversationStart,
} from "./conversationWindow";

type Props = {
  goal: GoalInfo | null;
  workspaceName: string | null;
  threadTitle: string | null;
  conversationId: string | null;
  threadLoading?: boolean;
  threadCreationStatus?: "creating" | "failed" | null;
  threadCreationError?: string | null;
  onRetryThreadCreation?: () => void;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
  rightPanelOpen?: boolean;
  activeRightPanelTab?: "agents" | "files";
  agentPanelAvailable?: boolean;
  agentPanelUnread?: boolean;
  onOpenAgentPanel?: () => void;
  onOpenFilePanel?: () => void;
  onOpenFile?: (path: string) => void;
  tokenUsage: import("../../types").ThreadTokenUsage | null;
  threadStatus: string;
  threadSettings: Record<string, unknown> | null;
  providers?: ModelProviderSummary[];
  currentProviderId?: string | null;
  models?: ModelSummary[];
  catalogLoading?: boolean;
  catalogError?: string | null;
  onRefreshCatalog?: () => void;
  onWriteProvider?: (input: Record<string, unknown>) => Promise<void>;
  onSelectProvider?: (providerId: string) => void;
  selectedModelId?: string | null;
  onSelectModel?: (modelId: string) => void;
  providerCatalogOpenRequest?: number;
  messages: MessageEntry[];
  finalArtifacts?: ArtifactSummary[];
  taskApprovals?: TaskApprovalRequest[];
  workspaceId?: string;
  thinking?: boolean;
  turnStartedAt?: number | null;
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
  pendingUserInputRequests: PendingUserInputSummary[];
  submittingPendingUserInputIds?: Set<string>;
  onSubmitPendingUserInput: (
    requestId: string,
    version: number,
    answers: Record<string, { answers: string[] }>,
  ) => Promise<void> | void;
  pendingMcpFormRequests: PendingMcpFormSummary[];
  submittingMcpFormIds?: Set<string>;
  onSubmitMcpForm: (
    requestId: string,
    version: number,
    action: McpFormResponseAction,
    content?: McpFormContent,
  ) => Promise<void> | void;
  busy: boolean;
  sendDisabled: boolean;
  onResolveApproval?: (workspaceId: string, requestId: number | string, decision: "accept" | "decline") => void;
};

function FinalArtifactLinks({ artifacts }: { artifacts: ArtifactSummary[] }) {
  const ready = artifacts.filter((artifact) => (
    artifact.state === "ready" && Boolean(artifact.download_url)
  ));
  if (ready.length === 0) return null;
  return (
    <section className="web-final-artifact-links" aria-label="生成的文件">
      {ready.map((artifact) => (
        <p key={artifact.id}>
          <span>{artifact.mime_type === "text/markdown" ? "正式简报" : "生成文件"}：</span>
          <a href={artifact.download_url ?? undefined} download>
            {artifact.mime_type === "text/markdown" ? "下载 Markdown 文件" : "下载文件"}
          </a>
        </p>
      ))}
    </section>
  );
}

export default function Conversation({
  goal,
  workspaceName,
  threadTitle,
  conversationId,
  threadLoading = false,
  threadCreationStatus = null,
  threadCreationError = null,
  onRetryThreadCreation,
  sidebarCollapsed,
  onToggleSidebar,
  rightPanelOpen = false,
  activeRightPanelTab = "files",
  agentPanelAvailable = false,
  agentPanelUnread = false,
  onOpenAgentPanel,
  onOpenFilePanel,
  onOpenFile,
  tokenUsage,
  threadStatus,
  threadSettings,
  providers,
  currentProviderId,
  models,
  catalogLoading,
  catalogError,
  onRefreshCatalog,
  onWriteProvider,
  onSelectProvider,
  selectedModelId,
  onSelectModel,
  providerCatalogOpenRequest,
  messages,
  finalArtifacts = [],
  taskApprovals = [],
  workspaceId,
  thinking,
  turnStartedAt,
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
  pendingUserInputRequests = [],
  submittingPendingUserInputIds = new Set(),
  onSubmitPendingUserInput,
  pendingMcpFormRequests = [],
  submittingMcpFormIds = new Set(),
  onSubmitMcpForm,
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
  }, [conversationId, finalArtifacts.length, messages, visibleStart]);

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
      <Header
        workspaceName={workspaceName}
        threadTitle={threadTitle}
        threadStatus={threadStatus}
        threadSettings={threadSettings}
        sidebarCollapsed={sidebarCollapsed}
        onToggleSidebar={onToggleSidebar}
        rightPanelOpen={rightPanelOpen}
        activeRightPanelTab={activeRightPanelTab}
        agentPanelAvailable={agentPanelAvailable}
        agentPanelUnread={agentPanelUnread}
        onOpenAgentPanel={onOpenAgentPanel}
        onOpenFilePanel={onOpenFilePanel}
      />
      <div
        className={`web-message-area${threadLoading ? " is-thread-loading" : ""}`}
        ref={messageAreaRef}
        onScroll={handleScroll}
      >
        {threadLoading || threadCreationStatus === "creating" ? (
          <div className="web-thread-loading" role="status" aria-live="polite">
            <span className="web-thread-loading-spinner" aria-hidden="true" />
            <strong>
              {threadCreationStatus === "creating" ? "正在创建 Thread…" : "正在加载 Thread…"}
            </strong>
            <span>
              {threadCreationStatus === "creating"
                ? "窗口已准备好，正在等待服务端完成绑定。"
                : "历史记录准备完成后会一次性显示。"}
            </span>
          </div>
        ) : threadCreationStatus === "failed" ? (
          <div className="web-thread-creation-error" role="alert">
            <strong>创建 Thread 失败</strong>
            <span>{threadCreationError || "服务端没有完成创建，请重试。"}</span>
            <button type="button" onClick={onRetryThreadCreation}>
              重试
            </button>
          </div>
        ) : null}
        <div
          className={`web-thread-content${threadLoading || threadCreationStatus ? " is-hidden" : ""}`}
          aria-hidden={threadLoading || Boolean(threadCreationStatus)}
        >
          {hasOlderMessages && (
            <button type="button" className="web-load-older" onClick={loadOlderMessages}>
              Load previous messages
            </button>
          )}
          <MessageList
            items={visibleMessages}
            thinking={thinking}
            turnStartedAt={turnStartedAt}
            onOpenFile={onOpenFile}
            workspaceId={workspaceId}
            onResolveApproval={onResolveApproval}
            inlineVisualizationThreadId={conversationId}
            onOpenAgentPanel={onOpenAgentPanel}
          />
          <FinalArtifactLinks artifacts={finalArtifacts} />
          <TaskApprovalQueue
            approvals={taskApprovals}
            ariaLabel="Task approvals"
            onResolve={onResolveApproval}
          />
          <UserInputQueue
            requests={pendingUserInputRequests}
            submittingIds={submittingPendingUserInputIds}
            onSubmit={onSubmitPendingUserInput}
          />
          <McpFormQueue
            requests={pendingMcpFormRequests}
            submittingIds={submittingMcpFormIds}
            onSubmit={onSubmitMcpForm}
          />
          {thinking && threadStatus !== "reconnecting" && !visibleMessages.some((entry) => entry.level === "user") && <ThinkingIndicator />}
        </div>
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
        disabled={sendDisabled || threadLoading || Boolean(threadCreationStatus)}
        tokenUsage={tokenUsage}
        providers={providers}
        currentProviderId={currentProviderId}
        models={models}
        catalogLoading={catalogLoading}
        catalogError={catalogError}
        onRefreshCatalog={onRefreshCatalog}
        onWriteProvider={onWriteProvider}
        onSelectProvider={onSelectProvider}
        selectedModelId={selectedModelId}
        onSelectModel={onSelectModel}
        providerCatalogOpenRequest={providerCatalogOpenRequest}
      />
    </section>
  );
}
