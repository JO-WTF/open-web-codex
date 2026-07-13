import { useRef, useEffect } from "react";
import type { CSSProperties } from "react";
import type { ThreadTokenUsage } from "../../types";
import Bot from "lucide-react/dist/esm/icons/bot";
import ArrowUp from "lucide-react/dist/esm/icons/arrow-up";
import ChevronDown from "lucide-react/dist/esm/icons/chevron-down";
import CircleStop from "lucide-react/dist/esm/icons/circle-stop";
import Gauge from "lucide-react/dist/esm/icons/gauge";
import ImagePlus from "lucide-react/dist/esm/icons/image-plus";
import ShieldCheck from "lucide-react/dist/esm/icons/shield-check";

type Props = {
  draft: string;
  onDraftChange: (text: string) => void;
  onSend: () => void;
  onStop: () => void;
  running: boolean;
  stopping: boolean;
  busy: boolean;
  disabled: boolean;
  tokenUsage: ThreadTokenUsage | null;
};

function formatTokens(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

export default function Composer({ draft, onDraftChange, onSend, onStop, running, stopping, busy, disabled, tokenUsage }: Props) {
  const textRef = useRef<HTMLTextAreaElement>(null);
  const composingRef = useRef(false);

  useEffect(() => {
    if (textRef.current) {
      textRef.current.style.height = "auto";
      textRef.current.style.height = `${Math.min(textRef.current.scrollHeight, 160)}px`;
    }
  }, [draft]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (running) {
      if (!stopping) onStop();
      return;
    }
    if (!disabled && !busy) onSend();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (composingRef.current || e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) {
      return;
    }
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (running) {
        if (draft.trim()) onSend();
        return;
      }
      handleSubmit(e);
    }
  };

  const contextWindow = tokenUsage?.modelContextWindow ?? 0;
  const lastTokens = tokenUsage?.last.totalTokens ?? 0;
  const contextUsage = tokenUsage
    ? lastTokens > 0 ? tokenUsage.last : tokenUsage.total
    : null;
  const usedTokens = contextUsage?.totalTokens ?? 0;
  const usedPercent = contextWindow > 0
    ? Math.min(Math.max((usedTokens / contextWindow) * 100, 0), 100)
    : null;
  const roundedPercent = usedPercent === null ? null : Math.round(usedPercent);
  const contextColor = usedPercent === null
    ? "#687183"
    : usedPercent >= 90
      ? "#ef6464"
      : usedPercent >= 70
        ? "#e9a04a"
        : "#82e63e";
  const contextLabel = usedPercent === null
    ? "Context usage unavailable"
    : `Context used ${roundedPercent}%: ${formatTokens(usedTokens)} of ${formatTokens(contextWindow)} tokens`;

  return (
    <form className="web-composer" onSubmit={handleSubmit}>
      <div className="web-composer-main">
        <span className="web-composer-utility" title="Image attachments are not available in Web mode" aria-hidden="true">
          <ImagePlus size={17} />
        </span>
        <div className="web-composer-inner">
          <textarea
            ref={textRef}
            value={draft}
            onChange={(e) => onDraftChange(e.target.value)}
            onKeyDown={handleKeyDown}
            onCompositionStart={() => {
              composingRef.current = true;
            }}
            onCompositionEnd={() => {
              composingRef.current = false;
            }}
            placeholder={running ? "Request follow-up changes…" : "Ask Codex to do something..."}
            disabled={busy}
            rows={1}
          />
        </div>
        <button
          type={running ? "button" : "submit"}
          className={`web-send-btn${running ? " is-stop" : ""}`}
          disabled={running ? stopping : disabled || busy}
          aria-label={running ? stopping ? "Stopping" : "Stop" : "Send"}
          onClick={running ? onStop : undefined}
        >
          {running ? (
            <CircleStop size={18} aria-hidden="true" />
          ) : (
            <ArrowUp size={20} strokeWidth={2.4} aria-hidden="true" />
          )}
        </button>
      </div>
      <div className="web-composer-footer" aria-label="Thread settings summary">
        <span className="web-composer-chip"><Bot size={13} />Codex<ChevronDown size={12} /></span>
        <span className="web-composer-chip"><Gauge size={13} />medium<ChevronDown size={12} /></span>
        <span className="web-composer-chip"><ShieldCheck size={13} />Workspace access<ChevronDown size={12} /></span>
        <span className="web-composer-activity" tabIndex={0} aria-label={contextLabel}>
          <span
            className="web-composer-activity-ring"
            style={{
              "--context-used": usedPercent ?? 0,
              "--context-color": contextColor,
            } as CSSProperties}
          />
          <span className="web-composer-context-tooltip" role="tooltip">
            <strong>{usedPercent === null ? "Context unavailable" : `Context used ${roundedPercent}%`}</strong>
            <span>
              {usedPercent === null
                ? "Waiting for token usage data"
                : `${formatTokens(usedTokens)} / ${formatTokens(contextWindow)} tokens`}
            </span>
            {contextUsage ? (
              <span>
                Input {formatTokens(contextUsage.inputTokens)} · Output {formatTokens(contextUsage.outputTokens)}
              </span>
            ) : null}
          </span>
        </span>
      </div>
    </form>
  );
}
