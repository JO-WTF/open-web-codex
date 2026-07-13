import { useEffect, useMemo, useState, type ReactNode } from "react";
import Brain from "lucide-react/dist/esm/icons/brain";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import type { MessageEntry } from "../MessageList";

type Props = {
  items: MessageEntry[];
  active: boolean;
  children: ReactNode;
};

export default function ExecutionGroup({ items, active, children }: Props) {
  const [open, setOpen] = useState(active);
  const [startedAt] = useState(Date.now);
  const [elapsed, setElapsed] = useState(0);
  const toolCount = useMemo(() => items.filter((item) => item.kind && item.kind !== "reasoning").length, [items]);
  const messageCount = useMemo(() => items.filter((item) => item.level === "assistant" || item.kind === "reasoning").length, [items]);

  useEffect(() => {
    if (!active) return;
    const update = () => setElapsed(Math.floor((Date.now() - startedAt) / 1000));
    update();
    const timer = window.setInterval(update, 1000);
    return () => window.clearInterval(timer);
  }, [active, startedAt]);

  useEffect(() => {
    // Live activity stays at the top level. Once the turn completes, collapse
    // that activity behind the summary while the final answer remains visible.
    setOpen(active);
  }, [active]);

  const elapsedLabel = `${Math.floor(elapsed / 60)}:${String(elapsed % 60).padStart(2, "0")}`;
  return (
    <section className={`web-execution-group${active ? " is-active" : ""}`}>
      {!active && (
        <button type="button" className="web-execution-summary" onClick={() => setOpen((value) => !value)} aria-expanded={open}>
          <ChevronRight size={12} className={open ? "is-open" : ""} />
          <span>{toolCount} tool {toolCount === 1 ? "call" : "calls"}, {messageCount} {messageCount === 1 ? "message" : "messages"}</span>
        </button>
      )}
      {(active || open) && <div className="web-execution-timeline">{children}</div>}
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
