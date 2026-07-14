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
            return <AssistantMessage key={entry.id} text={entry.text} streaming={entry.streaming} onOpenFile={onOpenFile} />;
          case "system":
            return <SystemNotice key={entry.id} text={entry.text} variant="default" />;
          case "error":
            return <SystemNotice key={entry.id} text={entry.text} variant="error" />;
          default:
            return <SystemNotice key={entry.id} text={entry.text} variant="neutral" />;
        }
  };

  const rendered: React.ReactNode[] = [];
  for (let index = 0; index < items.length;) {
    const entry = items[index];
    if (entry.level !== "user") {
      rendered.push(renderEntry(entry));
      index += 1;
      continue;
    }
    rendered.push(renderEntry(entry));
    let end = index + 1;
    while (end < items.length && items[end].level !== "user") end += 1;
    const turnItems = items.slice(index + 1, end);
    const hasLiveItem = turnItems.some((item) =>
      item.streaming
      || item.toolStatus === "inProgress"
      || item.toolStatus === "running"
    );
    const isActiveTurn = end === items.length && (thinking || hasLiveItem);
    let finalIndex = -1;
    if (!isActiveTurn) {
      for (let cursor = turnItems.length - 1; cursor >= 0; cursor -= 1) {
        if (turnItems[cursor].level === "assistant") { finalIndex = cursor; break; }
      }
    }
    let liveIndex = -1;
    if (isActiveTurn) {
      for (let cursor = turnItems.length - 1; cursor >= 0; cursor -= 1) {
        const item = turnItems[cursor];
        if (item.streaming || item.toolStatus === "inProgress" || item.toolStatus === "running") {
          liveIndex = cursor;
          break;
        }
      }
      if (liveIndex < 0 && turnItems.length > 0) liveIndex = turnItems.length - 1;
    }
    const executionItems = turnItems.filter((_, cursor) => cursor !== finalIndex && cursor !== liveIndex);
    const activeItem = liveIndex >= 0 ? turnItems[liveIndex] : null;
    if (executionItems.length > 0 || isActiveTurn) {
      rendered.push(
        <ExecutionGroup
          key={`execution-${entry.id}`}
          items={turnItems.filter((_, cursor) => cursor !== finalIndex)}
          active={isActiveTurn}
          startedAt={turnStartedAt}
          timelineItemCount={executionItems.length}
          activeItem={activeItem ? renderEntry(activeItem) : null}
        >
          {executionItems.map(renderEntry)}
        </ExecutionGroup>,
      );
    }
    if (finalIndex >= 0) rendered.push(renderEntry(turnItems[finalIndex]));
    index = end;
  }
  return <>{rendered}</>;
}
