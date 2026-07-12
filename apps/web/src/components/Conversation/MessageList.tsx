import type { LogEntry } from "../../WebApp";
import UserMessage from "./messages/UserMessage";
import AssistantMessage from "./messages/AssistantMessage";
import ReasoningBlock from "./messages/ReasoningBlock";
import ToolCallCard from "./messages/ToolCallCard";
import DiffBlock from "./messages/DiffBlock";
import SystemNotice from "./messages/SystemNotice";

type DiffLine = {
  type: "add" | "del" | "ctx";
  text: string;
};

// Re-export types used by the event handler
export type { DiffLine };

export type MessageEntry = LogEntry & {
  kind?: "reasoning" | "tool" | "diff";
  toolType?: string;
  toolTitle?: string;
  toolStatus?: string;
  filePath?: string;
  diffTitle?: string;
  diffLines?: DiffLine[];
  meta?: string;
  streaming?: boolean;
};

type Props = {
  items: MessageEntry[];
};

export default function MessageList({ items }: Props) {
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

  return (
    <>
      {items.map((entry) => {
        if (entry.kind === "reasoning") {
          return (
            <ReasoningBlock
              key={entry.id}
              text={entry.text}
              meta={entry.meta}
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
            />
          );
        }
        if (entry.kind === "diff") {
          return (
            <DiffBlock
              key={entry.id}
              title={entry.diffTitle ?? ""}
              lines={entry.diffLines ?? []}
            />
          );
        }
        switch (entry.level) {
          case "user":
            return <UserMessage key={entry.id} text={entry.text} />;
          case "assistant":
            return <AssistantMessage key={entry.id} text={entry.text} streaming={entry.streaming} />;
          case "system":
            return <SystemNotice key={entry.id} text={entry.text} variant="default" />;
          case "error":
            return <SystemNotice key={entry.id} text={entry.text} variant="error" />;
          default:
            return <SystemNotice key={entry.id} text={entry.text} variant="neutral" />;
        }
      })}
    </>
  );
}
