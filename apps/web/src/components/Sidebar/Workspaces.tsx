import { useState } from "react";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import Folder from "lucide-react/dist/esm/icons/folder";
import MessageSquare from "lucide-react/dist/esm/icons/message-square";
import Plus from "lucide-react/dist/esm/icons/plus";
import type { WorkspaceInfo } from "../../types";

type ThreadInfo = {
  id: string;
  label: string;
  updatedAt: number;
  turnCount?: number;
};

type Props = {
  workspaces: WorkspaceInfo[];
  activeId: string | null;
  onSelect: (id: string) => void;
  onCreate: (name: string) => void;
  onConnect: (id: string) => void;
  onLoad: () => void;
  busy: boolean;
  threadsByWorkspace: Record<string, ThreadInfo[]>;
  activeThreadId: string | null;
  onSelectThread: (id: string) => void;
  onNewThread: (workspaceId: string) => void;
};

function relativeTime(ts: number): string {
  const seconds = Math.floor((Date.now() - ts) / 1000);
  if (seconds < 60) return "now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.floor(hours / 24);
  return `${days}d`;
}

export default function Workspaces({
  workspaces,
  activeId,
  onSelect,
  onCreate,
  onConnect,
  busy,
  threadsByWorkspace,
  activeThreadId,
  onSelectThread,
  onNewThread,
}: Props) {
  const [createName, setCreateName] = useState("");
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const toggleExpand = (wsId: string) => {
    setExpandedId(prev => (prev === wsId ? null : wsId));
  };

  const threadsFor = (wsId: string): ThreadInfo[] => {
    return threadsByWorkspace[wsId] ?? [];
  };

  const handleCreate = () => {
    const name = createName.trim();
    if (name) { onCreate(name); setCreateName(""); }
  };

  return (
    <div className="web-ws-section">
      <div className="web-ws-header">
        <span className="web-ws-header-label">Workspaces</span>
        <span className="web-ws-header-count">{workspaces.length}</span>
      </div>

      <div className="web-ws-create">
        <input
          value={createName}
          onChange={(e) => setCreateName(e.target.value)}
          onKeyDown={(e) => { if (e.key === "Enter") handleCreate(); }}
          placeholder="New workspace..."
          disabled={busy}
          className="web-ws-create-input"
        />
        <button
          className="web-ws-create-btn"
          onClick={handleCreate}
          disabled={busy || !createName.trim()}
          title="Create workspace"
        >
          <Plus size={14} />
        </button>
      </div>

      {workspaces.length === 0 && (
        <div className="web-ws-empty">No workspaces</div>
      )}

      <div className="web-ws-list">
        {workspaces.map((ws) => {
          const isExpanded = expandedId === ws.id;
          const threads = threadsFor(ws.id);
          const isActive = ws.id === activeId;

          return (
            <div key={ws.id} className="web-ws-tree-node">
              {/* Workspace row */}
              <div
                className={`web-ws-row${isActive ? " web-ws-row-active" : ""}`}
                onClick={() => {
                  toggleExpand(ws.id);
                  if (!ws.connected) onConnect(ws.id);
                  onSelect(ws.id);
                }}
              >
                <span
                  className={`web-ws-arrow${isExpanded ? " web-ws-arrow-open" : ""}`}
                >
                  <ChevronRight size={12} />
                </span>
                <span
                  className={`web-ws-dot ${ws.connected ? "web-ws-dot-on" : "web-ws-dot-off"}`}
                />
                <Folder size={14} className="web-ws-folder-icon" />
                <span className="web-ws-name">{ws.name}</span>
                {threads.length > 0 && (
                  <span className="web-ws-thread-count">{threads.length}</span>
                )}
                <button
                  className="web-ws-new-thread-btn"
                  onClick={(e) => { e.stopPropagation(); setExpandedId(ws.id); onNewThread(ws.id); }}
                  disabled={busy}
                  title="New thread"
                >
                  <Plus size={12} />
                </button>
              </div>

              {/* Thread list (collapsible) */}
              {isExpanded && (
              <div className="web-ws-threads">
                {threads.length === 0 && (
                  <div className="web-ws-threads-empty">No threads</div>
                )}
                {threads.map((t) => (
                  <div
                    key={t.id}
                    className={`web-ws-thread${t.id === activeThreadId ? " web-ws-thread-active" : ""}`}
                    onClick={() => onSelectThread(t.id)}
                  >
                    <MessageSquare size={12} className="web-ws-thread-icon" />
                    <span className="web-ws-thread-label">{t.label}</span>
                    <span className="web-ws-thread-time">{relativeTime(t.updatedAt)}</span>
                  </div>
                ))}

              </div>)}
            </div>
          );
        })}
      </div>
    </div>
  );
}
