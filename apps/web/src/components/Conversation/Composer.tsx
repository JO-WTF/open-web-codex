import { useRef, useEffect, useState } from "react";
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
  providers?: ModelProviderSummary[];
  currentProviderId?: string | null;
  models?: ModelSummary[];
  catalogLoading?: boolean;
  catalogError?: string | null;
  onRefreshCatalog?: () => void;
};

export type ModelProviderSummary = {
  id: string;
  name: string;
  kind: "builtIn" | "local" | "custom";
  isCurrent: boolean;
  modelCount: number;
};

export type ModelSummary = {
  id: string;
  displayName: string;
  model: string;
};

function formatTokens(value: number): string {
  return new Intl.NumberFormat("en-US").format(value);
}

export default function Composer({ draft, onDraftChange, onSend, onStop, running, stopping, busy, disabled, tokenUsage, providers = [], currentProviderId = null, models = [], catalogLoading = false, catalogError = null, onRefreshCatalog }: Props) {
  const textRef = useRef<HTMLTextAreaElement>(null);
  const catalogRef = useRef<HTMLDivElement>(null);
  const composingRef = useRef(false);
  const [catalogOpen, setCatalogOpen] = useState(false);

  useEffect(() => {
    if (!catalogOpen) return;
    const closeOnOutsideClick = (event: MouseEvent) => {
      if (!catalogRef.current?.contains(event.target as Node)) setCatalogOpen(false);
    };
    window.addEventListener("mousedown", closeOnOutsideClick);
    return () => window.removeEventListener("mousedown", closeOnOutsideClick);
  }, [catalogOpen]);

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
        <div className="web-composer-catalog" ref={catalogRef}>
          <button
            className="web-composer-chip web-composer-chip-button"
            type="button"
            aria-expanded={catalogOpen}
            aria-haspopup="dialog"
            onClick={() => setCatalogOpen((open) => !open)}
          >
            <Bot size={13} />
            {providers.find((provider) => provider.id === currentProviderId)?.name ?? "Codex"}
            <ChevronDown size={12} />
          </button>
          {catalogOpen ? (
            <section className="web-model-catalog" role="dialog" aria-label="Providers and models">
              <header>
                <div><strong>Provider & model</strong><span>Managed by this Codex Profile</span></div>
                <button type="button" onClick={onRefreshCatalog} disabled={catalogLoading}>Refresh</button>
              </header>
              {catalogError ? <p className="web-model-catalog-error">{catalogError}</p> : null}
              <div className="web-model-catalog-section">
                <h3>Providers</h3>
                {providers.length === 0 ? <p>{catalogLoading ? "Loading providers…" : "No providers available"}</p> : providers.map((provider) => (
                  <div className={`web-model-catalog-row${provider.isCurrent ? " is-current" : ""}`} key={provider.id}>
                    <span><strong>{provider.name}</strong><small>{provider.kind} · {provider.modelCount} models</small></span>
                    {provider.isCurrent ? <em>Current</em> : null}
                  </div>
                ))}
              </div>
              <div className="web-model-catalog-section">
                <h3>Models</h3>
                {models.length === 0 ? <p>{catalogLoading ? "Loading models…" : "No models returned by this provider"}</p> : models.map((model) => (
                  <div className="web-model-catalog-row" key={model.id}>
                    <span><strong>{model.displayName || model.model}</strong><small>{model.model}</small></span>
                  </div>
                ))}
              </div>
              <footer>Adding, editing and selecting providers is enabled in the next implementation step.</footer>
            </section>
          ) : null}
        </div>
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
