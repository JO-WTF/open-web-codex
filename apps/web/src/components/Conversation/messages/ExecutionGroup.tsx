import { useEffect, useMemo, useState, type ReactNode } from "react";
import Brain from "lucide-react/dist/esm/icons/brain";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import type { MessageEntry } from "../MessageList";
import AgentWaitCard from "./AgentWaitCard";
import type { AgentWaitHistoryUpdate } from "../../../utils/agentWaitUpdates";

type Props = {
  items: MessageEntry[];
  active: boolean;
  startedAt?: number | null;
  durationMs?: number;
  activeItem?: ReactNode;
  timelineItemCount?: number;
  activityLabel?: string;
  agentWaitStatus?: string;
  agentWaitUpdates?: AgentWaitHistoryUpdate[];
  onShowAgentActivity?: () => void;
  children: ReactNode;
};

export default function ExecutionGroup({
  items,
  active,
  startedAt,
  durationMs,
  activeItem,
  timelineItemCount = 0,
  activityLabel = "Working…",
  agentWaitStatus,
  agentWaitUpdates,
  onShowAgentActivity,
  children,
}: Props) {
  const [manuallyOpen, setManuallyOpen] = useState(false);
  const [fallbackStartedAt] = useState(Date.now);
  const [elapsed, setElapsed] = useState(0);
  // Live activity is always visible. Once it completes, derive the collapsed
  // state during that same render so historical details never remain expanded
  // for one paint while an effect catches up.
  const open = active || manuallyOpen;
  const toolCount = useMemo(() => items.filter((item) =>
    item.kind === "tool"
    || item.kind === "command_exec"
    || item.kind === "diff").length, [items]);
  const messageCount = useMemo(() => items.filter((item) => item.level === "assistant"
    || (item.kind === "reasoning"
      && (!/^(reasoning completed|reasoning in progress|reasoning)$/i.test(item.text.trim())
        || Boolean(item.reasoningSummary?.trim())))).length, [items]);

  useEffect(() => {
    if (!active) return;
    const effectiveStartedAt = startedAt ?? fallbackStartedAt;
    const update = () => setElapsed(Math.max(0, Math.floor((Date.now() - effectiveStartedAt) / 1000)));
    update();
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [active, fallbackStartedAt, startedAt]);

  const elapsedLabel = formatExecutionDuration(elapsed * 1000);
  const summaryDuration = active
    ? elapsedLabel
    : (durationMs === undefined ? null : formatExecutionDuration(durationMs));
  const countLabel = `${toolCount} tool ${toolCount === 1 ? "call" : "calls"}, ${messageCount} ${messageCount === 1 ? "message" : "messages"}`;
  return (
    <section className={`web-execution-group${active ? " is-active" : ""}`}>
      {(!active || timelineItemCount > 0) && (
        <button
          type="button"
          className="web-execution-summary"
          onClick={() => { if (!active) setManuallyOpen((value) => !value); }}
          aria-expanded={open}
          aria-disabled={active}
        >
          <ChevronRight size={12} className={open ? "is-open" : ""} />
          <span>{countLabel}{summaryDuration ? ` · ${summaryDuration}` : ""}</span>
        </button>
      )}
      {((active && timelineItemCount > 0) || (!active && open)) && <div className="web-execution-timeline">{children}</div>}
      {activeItem ? <div className="web-execution-current">{activeItem}</div> : null}
      {active && agentWaitStatus ? (
        <div className="web-execution-current">
          <AgentWaitCard
            status={agentWaitStatus}
            elapsedLabel={elapsedLabel}
            live
            agentUpdates={agentWaitUpdates}
            onShowAgentActivity={onShowAgentActivity}
          />
        </div>
      ) : null}
      {active ? (
        <div className="web-execution-working" role="status">
          <span className="web-thinking-spinner" aria-hidden="true" />
          <span className="web-execution-elapsed">{elapsedLabel}</span>
          <Brain size={14} aria-hidden="true" />
          <span>{activityLabel}</span>
        </div>
      ) : null}
    </section>
  );
}

export function formatExecutionDuration(durationMs: number): string {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const seconds = String(totalSeconds % 60).padStart(2, "0");
  const totalMinutes = Math.floor(totalSeconds / 60);
  if (totalMinutes < 60) return `${totalMinutes}:${seconds}`;
  const minutes = String(totalMinutes % 60).padStart(2, "0");
  return `${Math.floor(totalMinutes / 60)}:${minutes}:${seconds}`;
}
