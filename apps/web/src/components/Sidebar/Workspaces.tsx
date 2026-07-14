import { useState } from "react";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import Folder from "lucide-react/dist/esm/icons/folder";
import MessageSquare from "lucide-react/dist/esm/icons/message-square";
import Trash2 from "lucide-react/dist/esm/icons/trash-2";
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
  onRemoveWorkspace: (workspaceId: string) => void;
};

function compactThreadId(id: string): string {
  return id.length > 8 ? `${id.slice(0, 8)}…` : id;
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
  onRemoveWorkspace,
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
          <span className="web-ws-create-plus" aria-hidden="true" />
        </button>
      </div>

      {workspaces.length === 0 && (
        <div className="web-ws-empty">No workspaces</div>
      )}

      <div className="web-ws-list">
        {workspaces.map((ws) => {
          const isExpanded = expandedId === ws.id || (expandedId === null && ws.id === activeId);
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
                  type="button"
                  className="web-ws-row-action web-ws-remove-btn"
                  onClick={(event) => {
                    event.stopPropagation();
                    onRemoveWorkspace(ws.id);
                  }}
                  disabled={busy}
                  aria-label={`Remove workspace ${ws.name}`}
                  title="Remove workspace"
                >
                  <Trash2 size={13} aria-hidden="true" />
                </button>
                <button
                  type="button"
                  className="web-ws-row-action web-ws-new-thread-btn"
                  onClick={(e) => { e.stopPropagation(); setExpandedId(ws.id); onNewThread(ws.id); }}
                  disabled={busy}
                  aria-label={`New thread in ${ws.name}`}
                  title="New thread"
                >
                  <span className="web-ws-create-plus web-ws-create-plus-small" aria-hidden="true" />
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
                    <span className="web-ws-thread-time" title={t.id}>{compactThreadId(t.id)}</span>
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
