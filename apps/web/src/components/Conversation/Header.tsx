import type { ThreadTokenUsage } from "../../types";

type Props = {
  workspaceName: string | null;
  threadTitle: string | null;
  tokenUsage?: ThreadTokenUsage | null;
  threadStatus?: string;
  threadSettings?: Record<string, unknown> | null;
};

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function statusDotClass(status: string): string {
  switch (status) {
    case "active":
    case "running":
      return "web-status-dot-active";
    case "error":
      return "web-status-dot-error";
    case "idle":
    default:
      return "web-status-dot-idle";
  }
}

export default function Header({
  workspaceName,
  threadTitle,
  tokenUsage,
  threadStatus,
  threadSettings,
}: Props) {
  const showAny = workspaceName || threadTitle || tokenUsage || (threadStatus !== "idle");
  if (!showAny) return null;

  // Extract useful settings for display
  const ts = threadSettings as Record<string, unknown> | null | undefined;
  const rawSettings = ts?.threadSettings as Record<string, unknown> | null | undefined;
  const collabMode = rawSettings?.collaborationMode as Record<string, unknown> | null | undefined;
  const collabLabel = typeof collabMode?.mode === "string" ? collabMode.mode : null;
  const approvalPolicy = typeof rawSettings?.approvalPolicy === "string" ? rawSettings.approvalPolicy : null;

  return (
    <div className="web-chat-header">
      <div className="web-chat-header-left">
        {workspaceName && (
          <>
            <span>{workspaceName}</span>
            <span className="web-chat-header-sep">/</span>
          </>
        )}
        {threadTitle && <span>{threadTitle}</span>}
        {threadStatus && threadStatus !== "idle" && (
          <span className={`web-status-dot ${statusDotClass(threadStatus)}`} title={`Thread: ${threadStatus}`} />
        )}
      </div>
      <div className="web-chat-header-right">
        {collabLabel && (
          <span className="web-header-badge" title="Collaboration mode">
            {collabLabel}
          </span>
        )}
        {approvalPolicy && (
          <span className="web-header-badge" title="Approval policy">
            {approvalPolicy}
          </span>
        )}
        {tokenUsage && (
          <div className="web-chat-header-tokens" title={`Total tokens: ${tokenUsage.total.totalTokens.toLocaleString()}`}>
            <svg className="web-token-icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5">
              <circle cx="8" cy="8" r="6.5" />
              <path d="M5 8h6M8 5v6" />
            </svg>
            <span className="web-token-value">{formatTokens(tokenUsage.total.totalTokens)}</span>
            <span className="web-token-sep">·</span>
            <span className="web-token-label">in</span>
            <span className="web-token-value">{formatTokens(tokenUsage.total.inputTokens)}</span>
            <span className="web-token-sep">·</span>
            <span className="web-token-label">out</span>
            <span className="web-token-value">{formatTokens(tokenUsage.total.outputTokens)}</span>
          </div>
        )}
      </div>
    </div>
  );
}
