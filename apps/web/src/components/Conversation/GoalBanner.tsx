import { useState } from "react";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import Target from "lucide-react/dist/esm/icons/target";
import Clock from "lucide-react/dist/esm/icons/clock";
import Zap from "lucide-react/dist/esm/icons/zap";

export type GoalInfo = {
  objective: string;
  status: string;
  tokenBudget: number | null;
  tokensUsed: number;
  timeUsedSeconds: number;
};

type Props = {
  goal: GoalInfo | null;
};

function statusLabel(s: string): string {
  switch (s) {
    case "active": return "Active";
    case "paused": return "Paused";
    case "blocked": return "Blocked";
    case "usageLimited": return "Usage limited";
    case "budgetLimited": return "Budget limited";
    default: return s;
  }
}

function statusClass(s: string): string {
  switch (s) {
    case "active": return "web-goal-status-active";
    case "paused": return "web-goal-status-paused";
    case "blocked": return "web-goal-status-blocked";
    default: return "web-goal-status-other";
  }
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatTime(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

export default function GoalBanner({ goal }: Props) {
  const [open, setOpen] = useState(false);

  if (!goal) return null;

  const tokenPct = goal.tokenBudget ? Math.round((goal.tokensUsed / goal.tokenBudget) * 100) : null;

  return (
    <div className="web-goal-banner">
      {/* Collapsed header — always visible */}
      <div className="web-goal-header" onClick={() => setOpen(!open)}>
        <span className={`web-goal-chevron${open ? " web-goal-chevron-open" : ""}`}>
          <ChevronRight size={12} />
        </span>
        <Target size={14} className="web-goal-icon" />
        <span className="web-goal-objective">{goal.objective}</span>
        <span className={`web-goal-badge ${statusClass(goal.status)}`}>
          {statusLabel(goal.status)}
        </span>
        {tokenPct != null && (
          <span className="web-goal-progress" title={`${formatTokens(goal.tokensUsed)} / ${formatTokens(goal.tokenBudget!)} tokens`}>
            <span className="web-goal-progress-bar">
              <span className="web-goal-progress-fill" style={{ width: `${Math.min(tokenPct, 100)}%` }} />
            </span>
            <span className="web-goal-progress-text">{tokenPct}%</span>
          </span>
        )}
      </div>

      {/* Expanded detail — only when open */}
      {open && (
        <div className="web-goal-detail">
          <div className="web-goal-detail-row">
            <Clock size={12} className="web-goal-detail-icon" />
            <span className="web-goal-detail-label">Time used</span>
            <span className="web-goal-detail-value">{formatTime(goal.timeUsedSeconds)}</span>
          </div>
          <div className="web-goal-detail-row">
            <Zap size={12} className="web-goal-detail-icon" />
            <span className="web-goal-detail-label">Tokens used</span>
            <span className="web-goal-detail-value">
              {formatTokens(goal.tokensUsed)}
              {goal.tokenBudget && <span className="web-goal-detail-sub"> / {formatTokens(goal.tokenBudget)} budget</span>}
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
