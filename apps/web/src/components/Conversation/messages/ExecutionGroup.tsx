import { useEffect, useMemo, useState, type ReactNode } from "react";
import Brain from "lucide-react/dist/esm/icons/brain";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import type { MessageEntry } from "../MessageList";

type Props = {
  items: MessageEntry[];
  active: boolean;
  startedAt?: number | null;
  activeItem?: ReactNode;
  timelineItemCount?: number;
  children: ReactNode;
};

export default function ExecutionGroup({ items, active, startedAt, activeItem, timelineItemCount = 0, children }: Props) {
  const [open, setOpen] = useState(active);
  const [fallbackStartedAt] = useState(Date.now);
  const [elapsed, setElapsed] = useState(0);
  const toolCount = useMemo(() => items.filter((item) => item.kind && item.kind !== "reasoning").length, [items]);
  const messageCount = useMemo(() => items.filter((item) => item.level === "assistant" || item.kind === "reasoning").length, [items]);

  useEffect(() => {
    if (!active) return;
    const effectiveStartedAt = startedAt ?? fallbackStartedAt;
    const update = () => setElapsed(Math.max(0, Math.floor((Date.now() - effectiveStartedAt) / 1000)));
    update();
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [active, fallbackStartedAt, startedAt]);

  useEffect(() => {
    // Live activity stays at the top level. Once the turn completes, collapse
    // that activity behind the summary while the final answer remains visible.
    setOpen(active);
  }, [active]);

  const elapsedLabel = `${Math.floor(elapsed / 60)}:${String(elapsed % 60).padStart(2, "0")}`;
  return (
    <section className={`web-execution-group${active ? " is-active" : ""}`}>
      {(!active || timelineItemCount > 0) && (
        <button
          type="button"
          className="web-execution-summary"
          onClick={() => { if (!active) setOpen((value) => !value); }}
          aria-expanded={open}
          aria-disabled={active}
        >
          <ChevronRight size={12} className={open ? "is-open" : ""} />
          <span>{toolCount} tool {toolCount === 1 ? "call" : "calls"}, {messageCount} {messageCount === 1 ? "message" : "messages"}</span>
        </button>
      )}
      {((active && timelineItemCount > 0) || (!active && open)) && <div className="web-execution-timeline">{children}</div>}
      {activeItem ? <div className="web-execution-current">{activeItem}</div> : null}
      {active && (
        <div className="web-execution-working" role="status">
          <span className="web-thinking-spinner" aria-hidden="true" />
          <span className="web-execution-elapsed">{elapsedLabel}</span>
          <Brain size={14} aria-hidden="true" />
          <span>Working…</span>
        </div>
      )}
    </section>
  );
}
