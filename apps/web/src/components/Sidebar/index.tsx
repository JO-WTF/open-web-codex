import { useState } from "react";
import type { WorkspaceInfo } from "../../types";
import Brand from "./Brand";
import Workspaces from "./Workspaces";
import Threads from "./Threads";

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
  onAddWorkspace: (path: string) => void;
  threads: ThreadInfo[];
  activeThreadId: string | null;
  onSelectThread: (id: string) => void;
  onNewThread: () => void;
  baseUrl: string;
  token: string;
  onBaseUrlChange: (url: string) => void;
  onTokenChange: (token: string) => void;
  onCheckGateway: () => void;
  onLoadWorkspaces: () => void;
  busy: boolean;
};

export default function Sidebar({
  gatewayState,
  gatewayVersion,
  workspaces,
  activeWorkspaceId,
  onSelectWorkspace,
  onAddWorkspace,
  threads,
  activeThreadId,
  onSelectThread,
  onNewThread,
  baseUrl,
  token,
  onBaseUrlChange,
  onTokenChange,
  onCheckGateway,
  onLoadWorkspaces,
  busy,
}: Props) {
  const [showSettings, setShowSettings] = useState(false);

  return (
    <aside className="web-sidebar">
      <div className="web-sidebar-inner">
        <Brand state={gatewayState} version={gatewayVersion} />

        <div
          className="web-settings-toggle"
          onClick={() => setShowSettings(!showSettings)}
        >
          {showSettings ? "▼" : "▶"} Connection Settings
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
              <button
                className="web-settings-btn web-settings-btn-ghost"
                onClick={onLoadWorkspaces}
                disabled={busy}
              >
                Load
              </button>
            </div>
            {gatewayVersion && (
              <div className="web-version-info">Gateway v{gatewayVersion}</div>
            )}
          </div>
        )}

        <Workspaces
          workspaces={workspaces}
          activeId={activeWorkspaceId}
          onSelect={onSelectWorkspace}
          onAdd={onAddWorkspace}
          busy={busy}
        />

        <Threads
          threads={threads}
          activeId={activeThreadId}
          onSelect={onSelectThread}
          onNew={onNewThread}
          busy={busy}
        />
      </div>
    </aside>
  );
}
