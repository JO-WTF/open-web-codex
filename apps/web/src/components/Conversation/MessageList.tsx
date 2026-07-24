import type { LogEntry } from "../../WebApp";
import UserMessage from "./messages/UserMessage";
import AssistantMessage from "./messages/AssistantMessage";
import ReasoningBlock from "./messages/ReasoningBlock";
import ToolCallCard from "./messages/ToolCallCard";
import DiffBlock from "./messages/DiffBlock";
import ApprovalCard from "./messages/ApprovalCard";
import CommandExecutionCard from "./messages/CommandExecutionCard";
import SystemNotice from "./messages/SystemNotice";
import ExecutionGroup from "./messages/ExecutionGroup";
import ReplyCard from "./messages/ReplyCard";

type DiffLine = {
  type: "add" | "del" | "ctx";
  text: string;
};

// Re-export types used by the event handler
export type { DiffLine };

export type MessageEntry = LogEntry & {
  kind?: "reasoning" | "tool" | "diff" | "approval" | "command_exec" | "connection";
  toolType?: string;
  toolTitle?: string;
  toolStatus?: string;
  toolDetail?: string;
  toolOutput?: string;
  filePath?: string;
  diffTitle?: string;
  diffLines?: DiffLine[];
  meta?: string;
  streaming?: boolean;
};

type Props = {
  items: MessageEntry[];
  thinking?: boolean;
  turnStartedAt?: number | null;
  onOpenFile?: (path: string) => void;
  workspaceId?: string;
  onResolveApproval?: (workspaceId: string, requestId: number | string, decision: "accept" | "decline") => void;
};

function isAssistantProcessEntry(entry: MessageEntry) {
  return entry.level === "assistant"
    && (
      entry.messagePhase === "commentary"
      || (entry.streaming && entry.messagePhase === undefined)
    );
}

export default function MessageList({ items, thinking = false, turnStartedAt, onOpenFile, workspaceId, onResolveApproval }: Props) {
  if (items.length === 0) {
    return (
      <div className="web-empty">
        <div className="web-empty-inner">
          <div className="web-brand-sub">Ready when you are</div>
          <h2>Start a Codex task from the browser</h2>
          <p>
            Select a workspace, start a thread, and describe the task.
            Events and messages will appear here in real time.
          </p>
        </div>
      </div>
    );
  }

  const renderEntry = (entry: MessageEntry) => {
        if (entry.kind === "connection") {
          return (
            <div className="web-connection-notice" role="status" key={entry.id}>
              <span className="web-thinking-spinner" aria-hidden="true" />
              <span>{entry.text}</span>
            </div>
          );
        }

        if (entry.kind === "approval") {
          return (
            <ApprovalCard
              key={entry.id}
              command={entry.text}
              workspaceId={workspaceId}
              requestId={entry.approvalRequestId}
              status={entry.approvalStatus}
              mode={entry.approvalMode}
              url={entry.approvalUrl}
              serverName={entry.approvalServerName}
              onResolve={onResolveApproval}
            />
          );
        }

        if (entry.kind === "command_exec") {
          return (
            <CommandExecutionCard
              key={entry.id}
              command={entry.text}
              output={entry.cmdOutput ?? void 0}
              exitCode={entry.cmdExitCode}
              status={entry.toolStatus}
              durationMs={entry.cmdDurationMs}
              cwd={entry.cmdCwd}
              commandActions={entry.cmdActions}
              approvalStatus={entry.approvalStatus}
            />
          );
        }

        if (entry.kind === "reasoning") {
          return (
            <ReasoningBlock
              key={entry.id}
              text={entry.text}
              summary={entry.reasoningSummary}
              meta={entry.meta}
              streaming={entry.streaming}
            />
          );
        }
        if (entry.kind === "tool") {
          return (
            <ToolCallCard
              key={entry.id}
              toolType={entry.toolType ?? ""}
              title={entry.toolTitle ?? ""}
              status={entry.toolStatus ?? ""}
              filePath={entry.filePath}
              detail={entry.toolDetail}
              output={entry.toolOutput}
            />
          );
        }
        if (entry.kind === "diff") {
          return (
            <DiffBlock
              key={entry.id}
              title={entry.diffTitle ?? ""}
              lines={entry.diffLines ?? []}
              updating={entry.streaming}
            />
          );
        }
        switch (entry.level) {
          case "user":
            return <UserMessage key={entry.id} text={entry.text} />;
          case "assistant":
            return (
              <AssistantMessage
                key={entry.id}
                text={entry.text}
                streaming={entry.streaming}
                onOpenFile={onOpenFile}
                variant={isAssistantProcessEntry(entry) ? "commentary" : "reply"}
              />
            );
          case "system":
            return <SystemNotice key={entry.id} text={entry.text} variant="default" />;
          case "error":
            return <SystemNotice key={entry.id} text={entry.text} variant="error" />;
          default:
            return <SystemNotice key={entry.id} text={entry.text} variant="neutral" />;
        }
  };

  const rendered: React.ReactNode[] = [];
  const isLiveEntry = (entry: MessageEntry) =>
    entry.streaming
    || entry.toolStatus === "inProgress"
    || entry.toolStatus === "running";
  const isAssistantReply = (entry: MessageEntry) =>
    entry.level === "assistant" && !isAssistantProcessEntry(entry);
  const appendReplyCard = (entry: MessageEntry) => {
    if (entry.replyCard) {
      rendered.push(<ReplyCard key={`${entry.id}-reply-card`} card={entry.replyCard} />);
    }
  };

  for (let index = 0; index < items.length;) {
    const entry = items[index];
    if (entry.level !== "user") {
      rendered.push(renderEntry(entry));
      appendReplyCard(entry);
      index += 1;
      continue;
    }
    rendered.push(renderEntry(entry));
    let end = index + 1;
    while (end < items.length && items[end].level !== "user") end += 1;
    const turnItems = items.slice(index + 1, end);
    const hasLiveItem = turnItems.some(isLiveEntry);
    const pendingApproval = turnItems.find((item) => {
      if (item.kind !== "approval") return false;
      if (item.approvalStatus === "pending") return true;
      const mapsCredentialWasDelivered = item.approvalMode === "url"
        && (
          item.approvalServerName === "map_utils"
          || item.approvalServerName === "workspace_maps"
        )
        && (
          /maps provider and api key/i.test(item.text)
          || /(?:google maps|mapbox(?: maps)?)\s+(?:api key|access token)/i.test(item.text)
        );
      return item.approvalMode === "url"
        && item.approvalStatus === "accepted"
        && !mapsCredentialWasDelivered;
    });
    const activityLabel = pendingApproval?.approvalMode === "url"
      ? "Waiting for API key…"
      : pendingApproval
        ? "Waiting for approval…"
        : "Working…";
    const isActiveTurn = end === items.length && (thinking || hasLiveItem);

    let executionSegment: MessageEntry[] = [];
    let executionSegmentIndex = 0;
    const flushExecutionSegment = (active: boolean) => {
      if (executionSegment.length === 0 && !active) return;
      let activeIndex = -1;
      if (active) {
        for (let cursor = executionSegment.length - 1; cursor >= 0; cursor -= 1) {
          if (isLiveEntry(executionSegment[cursor])) {
            activeIndex = cursor;
            break;
          }
        }
        if (activeIndex < 0 && executionSegment.length > 0) {
          activeIndex = executionSegment.length - 1;
        }
      }
      const timelineItems = executionSegment.filter((_, cursor) => cursor !== activeIndex);
      const activeItem = activeIndex >= 0 ? executionSegment[activeIndex] : null;
      rendered.push(
        <ExecutionGroup
          key={`execution-${entry.id}-${executionSegmentIndex}`}
          items={executionSegment}
          active={active}
          startedAt={turnStartedAt}
          timelineItemCount={timelineItems.length}
          activeItem={activeItem ? renderEntry(activeItem) : null}
          activityLabel={activityLabel}
        >
          {timelineItems.map(renderEntry)}
        </ExecutionGroup>,
      );
      executionSegment = [];
      executionSegmentIndex += 1;
    };

    for (const item of turnItems) {
      if (isAssistantReply(item)) {
        flushExecutionSegment(false);
        rendered.push(renderEntry(item));
        appendReplyCard(item);
        continue;
      }
      executionSegment.push(item);
      if (item.replyCard) {
        flushExecutionSegment(false);
        appendReplyCard(item);
      }
    }

    if (executionSegment.length > 0) {
      flushExecutionSegment(isActiveTurn);
    } else {
      const lastItem = turnItems[turnItems.length - 1];
      if (isActiveTurn && (!lastItem || !isAssistantReply(lastItem))) {
        flushExecutionSegment(true);
      }
    }
    index = end;
  }
  return <>{rendered}</>;
}
