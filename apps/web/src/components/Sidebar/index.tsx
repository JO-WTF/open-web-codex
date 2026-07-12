 import { useState } from "react";
 import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
 import type { WorkspaceInfo } from "../../types";
import Brand from "./Brand";
import Workspaces from "./Workspaces";
import McpStatus from "./McpStatus";

type ThreadInfo = {
  id: string;
  label: string;
  updatedAt: number;
  turnCount?: number;
};

type Props = {
  gatewayState: "checking" | "online" | "offline";
  gatewayVersion: string | null;
  workspaces: WorkspaceInfo[];
  activeWorkspaceId: string | null;
  onSelectWorkspace: (id: string) => void;
  onCreateWorkspace: (name: string) => void;
  onLoadWorkspaces: () => void;
  onConnectWorkspace: (id: string) => void;
  threadsByWorkspace: Record<string, ThreadInfo[]>;
  activeThreadId: string | null;
  onSelectThread: (id: string) => void;
  onNewThread: (workspaceId: string) => void;
  baseUrl: string;
  token: string;
  onBaseUrlChange: (url: string) => void;
  onTokenChange: (token: string) => void;
  onCheckGateway: () => void;
  mcpServers: Record<string, {name: string; status: string; error?: string | null; failureReason?: string | null}>;
  rateLimits: Record<string, unknown> | null;
  busy: boolean;
};

export default function Sidebar({
  gatewayState,
  gatewayVersion,
  workspaces,
  activeWorkspaceId,
  onSelectWorkspace,
  onCreateWorkspace,
  onLoadWorkspaces,
  onConnectWorkspace,
  threadsByWorkspace,
  activeThreadId,
  onSelectThread,
  onNewThread,
  baseUrl,
  token,
  onBaseUrlChange,
  onTokenChange,
  onCheckGateway,
  mcpServers,
  rateLimits,
  busy,
}: Props) {
  const [showSettings, setShowSettings] = useState(false);

  return (
    <aside className="web-sidebar">
      <div className="web-sidebar-scroll">
        <Brand state={gatewayState} version={gatewayVersion} />

        {rateLimits && (() => {
          const rl = rateLimits as Record<string, unknown>;
          const primary = rl.primary as Record<string, unknown> | null | undefined;
          const usedPct = primary?.usedPercent != null ? Math.round(primary.usedPercent as number) : null;
          const planType = typeof rl.planType === "string" ? rl.planType : null;
          return (
            <div className="web-rate-limit">
              {usedPct != null && (
                <span className="web-rate-pct" title={`Rate limit: ${usedPct}% used`}>
                  <span className="web-rate-bar">
                    <span className="web-rate-fill" style={{width: `${Math.min(usedPct, 100)}%`}} />
                  </span>
                  <span className="web-rate-text">{usedPct}%</span>
                </span>
              )}
              {planType && <span className="web-rate-plan">{planType}</span>}
            </div>
          );
        })()}

        <Workspaces
          workspaces={workspaces}
          activeId={activeWorkspaceId}
          onSelect={onSelectWorkspace}
          onCreate={onCreateWorkspace}
          onConnect={onConnectWorkspace}
          onLoad={onLoadWorkspaces}
          threadsByWorkspace={threadsByWorkspace}
          activeThreadId={activeThreadId}
          onSelectThread={onSelectThread}
          onNewThread={onNewThread}
          busy={busy}
        />

        <McpStatus servers={mcpServers} />


      </div>

      <div className="web-sidebar-bottom">
        <div
          className="web-settings-toggle"
          onClick={() => setShowSettings(!showSettings)}
        >
          <span className={`web-settings-toggle-icon${showSettings ? " web-settings-icon-open" : ""}`}>
            <ChevronRight size={10} />
          </span>
          Settings
        </div>
        {showSettings && (
          <div className="web-settings-panel">
            <label>
              Gateway URL
              <input
                value={baseUrl}
                onChange={(e) => onBaseUrlChange(e.target.value)}
                placeholder="http://127.0.0.1:4733"
              />
            </label>
            <label>
              Token
              <input
                value={token}
                onChange={(e) => onTokenChange(e.target.value)}
                type="password"
                placeholder="Optional auth token"
              />
            </label>
            <div className="web-settings-actions">
              <button
                className="web-settings-btn web-settings-btn-primary"
                onClick={onCheckGateway}
                disabled={busy}
              >
                Save & Check
              </button>
            </div>
            {gatewayVersion && (
              <div className="web-version-info">Gateway v{gatewayVersion}</div>
            )}
          </div>
        )}
      </div>
    </aside>
  );
}
