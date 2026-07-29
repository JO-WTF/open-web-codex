import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import Archive from "lucide-react/dist/esm/icons/archive";
import Bot from "lucide-react/dist/esm/icons/bot";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import Folder from "lucide-react/dist/esm/icons/folder";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle";
import MessageSquare from "lucide-react/dist/esm/icons/message-square";
import Sparkles from "lucide-react/dist/esm/icons/sparkles";
import Trash2 from "lucide-react/dist/esm/icons/trash-2";
import type {
  AgentDefinitionSummary,
  SupervisorPolicySummary,
} from "../../../browser/types";
import type { WorkspaceInfo } from "../../types";

type ThreadInfo = {
  id: string;
  label: string;
  updatedAt: number;
  turnCount?: number;
  status?: string;
  creationStatus?: "creating" | "failed";
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
  supervisorPolicies?: SupervisorPolicySummary[];
  supervisorPoliciesLoading?: boolean;
  supervisorPoliciesError?: string | null;
  onNewSupervisor?: (workspaceId: string, policy: SupervisorPolicySummary) => void;
  agents?: AgentDefinitionSummary[];
  agentsLoading?: boolean;
  agentsError?: string | null;
  onNewAgent?: (workspaceId: string, agent: AgentDefinitionSummary) => void;
  onArchiveThread: (workspaceId: string, threadId: string) => void;
  onRemoveWorkspace: (workspaceId: string) => void;
};

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
  supervisorPolicies = [],
  supervisorPoliciesLoading = false,
  supervisorPoliciesError = null,
  onNewSupervisor,
  agents = [],
  agentsLoading = false,
  agentsError = null,
  onNewAgent,
  onArchiveThread,
  onRemoveWorkspace,
}: Props) {
  const [createName, setCreateName] = useState("");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [pendingArchive, setPendingArchive] = useState<{
    workspaceId: string;
    threadId: string;
    label: string;
  } | null>(null);
  const [pendingSupervisor, setPendingSupervisor] = useState<{
    workspaceId: string;
    workspaceName: string;
  } | null>(null);
  const [pendingAgent, setPendingAgent] = useState<{
    workspaceId: string;
    workspaceName: string;
  } | null>(null);

  useEffect(() => {
    if (!pendingArchive && !pendingSupervisor && !pendingAgent) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setPendingArchive(null);
      setPendingSupervisor(null);
      setPendingAgent(null);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [pendingAgent, pendingArchive, pendingSupervisor]);

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

  const compatibleAgents = pendingAgent
    ? agents.filter((agent) =>
        agent.required_workspace_id === null
        || agent.required_workspace_id === pendingAgent.workspaceId)
    : [];

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
                {onNewAgent ? (
                  <button
                    type="button"
                    className="web-ws-row-action web-ws-new-agent-btn"
                    onClick={(event) => {
                      event.stopPropagation();
                      setExpandedId(ws.id);
                      setPendingAgent({
                        workspaceId: ws.id,
                        workspaceName: ws.name,
                      });
                    }}
                    disabled={busy}
                    aria-label={`Choose agent in ${ws.name}`}
                    title="Start governed agent"
                  >
                    <Bot size={13} aria-hidden="true" />
                  </button>
                ) : null}
                {onNewSupervisor ? (
                  <button
                    type="button"
                    className="web-ws-row-action web-ws-new-supervisor-btn"
                    onClick={(event) => {
                      event.stopPropagation();
                      setExpandedId(ws.id);
                      setPendingSupervisor({
                        workspaceId: ws.id,
                        workspaceName: ws.name,
                      });
                    }}
                    disabled={busy}
                    aria-label={`Choose supervisor policy in ${ws.name}`}
                    title="Start governed supervisor"
                  >
                    <Sparkles size={13} aria-hidden="true" />
                  </button>
                ) : null}
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
                {threads.map((t) => {
                  const isRunning = t.creationStatus === "creating"
                    || t.status === "active"
                    || t.status === "running"
                    || t.status === "reconnecting";
                  const isFailed = t.creationStatus === "failed";

                  return (
                    <div
                      key={t.id}
                      className={`web-ws-thread${t.id === activeThreadId ? " web-ws-thread-active" : ""}`}
                      onClick={() => onSelectThread(t.id)}
                    >
                      <span
                        className={`web-ws-thread-status${isFailed ? " is-failed" : ""}`}
                        aria-label={isFailed ? "Creation failed" : "Thread"}
                      >
                        <MessageSquare size={12} className="web-ws-thread-icon" />
                      </span>
                      <span className="web-ws-thread-label">{t.label}</span>
                      {isRunning && (
                        <span
                          className="web-ws-thread-running"
                          role="status"
                          aria-label={t.creationStatus === "creating" ? "Creating thread" : "Thread is running"}
                          title={t.creationStatus === "creating" ? "Creating thread" : "Thread is running"}
                        >
                          <LoaderCircle size={13} aria-hidden="true" />
                        </span>
                      )}
                      <button
                        type="button"
                        className="web-ws-thread-archive"
                        aria-label={`Archive thread ${t.label}`}
                        title="Archive thread"
                        disabled={busy || Boolean(t.creationStatus) || isRunning}
                        onClick={(event) => {
                          event.stopPropagation();
                          setPendingArchive({ workspaceId: ws.id, threadId: t.id, label: t.label });
                        }}
                      >
                        <Archive size={12} aria-hidden="true" />
                      </button>
                    </div>
                  );
                })}

              </div>)}
            </div>
          );
        })}
      </div>
      {pendingAgent && createPortal(
        <div
          className="web-settings-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setPendingAgent(null);
          }}
        >
          <section
            className="web-supervisor-policy-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="web-agent-run-title"
            aria-describedby="web-agent-run-description"
          >
            <div className="web-supervisor-policy-heading">
              <div className="web-supervisor-policy-icon">
                <Bot size={18} aria-hidden="true" />
              </div>
              <div>
                <h2 id="web-agent-run-title">Start governed agent</h2>
                <p id="web-agent-run-description">
                  Choose a published Agent compatible with {pendingAgent.workspaceName}.
                </p>
              </div>
            </div>
            <div className="web-supervisor-policy-list">
              {agentsLoading ? (
                <div className="web-supervisor-policy-empty" role="status">
                  Loading published Agents...
                </div>
              ) : agentsError ? (
                <div className="web-supervisor-policy-empty" role="alert">
                  Agent catalog is unavailable.
                </div>
              ) : compatibleAgents.length === 0 ? (
                <div className="web-supervisor-policy-empty">
                  No published Agents are compatible with this Workspace.
                </div>
              ) : compatibleAgents.map((agent) => (
                <button
                  type="button"
                  className="web-supervisor-policy-option"
                  key={agent.release_id
                    ?? `repository:${agent.definition_id}@${agent.version}`}
                  disabled={busy}
                  onClick={() => {
                    onNewAgent?.(pendingAgent.workspaceId, agent);
                    setPendingAgent(null);
                  }}
                >
                  <span className="web-supervisor-policy-option-title">
                    {agent.display_name}
                  </span>
                  <span className="web-supervisor-policy-option-version">
                    {agent.version}
                  </span>
                  <span className="web-supervisor-policy-option-description">
                    {agent.description}
                  </span>
                </button>
              ))}
            </div>
            <div className="web-supervisor-policy-actions">
              <button type="button" onClick={() => setPendingAgent(null)}>
                Cancel
              </button>
            </div>
          </section>
        </div>,
        document.body,
      )}
      {pendingSupervisor && createPortal(
        <div
          className="web-settings-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setPendingSupervisor(null);
          }}
        >
          <section
            className="web-supervisor-policy-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="web-supervisor-policy-title"
            aria-describedby="web-supervisor-policy-description"
          >
            <div className="web-supervisor-policy-heading">
              <div className="web-supervisor-policy-icon">
                <Sparkles size={18} aria-hidden="true" />
              </div>
              <div>
                <h2 id="web-supervisor-policy-title">Start governed supervisor</h2>
                <p id="web-supervisor-policy-description">
                  Choose a published policy for {pendingSupervisor.workspaceName}.
                </p>
              </div>
            </div>
            <div className="web-supervisor-policy-list">
              {supervisorPoliciesLoading ? (
                <div className="web-supervisor-policy-empty" role="status">
                  Loading published policies...
                </div>
              ) : supervisorPoliciesError ? (
                <div className="web-supervisor-policy-empty" role="alert">
                  Supervisor Policy catalog is unavailable.
                </div>
              ) : supervisorPolicies.length === 0 ? (
                <div className="web-supervisor-policy-empty">
                  No Supervisor Policies are published for new Runs.
                </div>
              ) : supervisorPolicies.map((policy) => (
                <button
                  type="button"
                  className="web-supervisor-policy-option"
                  key={`${policy.policy_id}@${policy.version}`}
                  disabled={busy}
                  onClick={() => {
                    onNewSupervisor?.(pendingSupervisor.workspaceId, policy);
                    setPendingSupervisor(null);
                  }}
                >
                  <span className="web-supervisor-policy-option-title">
                    {policy.display_name}
                  </span>
                  <span className="web-supervisor-policy-option-version">
                    {policy.version}
                  </span>
                  <span className="web-supervisor-policy-option-description">
                    {policy.description}
                  </span>
                </button>
              ))}
            </div>
            <div className="web-supervisor-policy-actions">
              <button type="button" onClick={() => setPendingSupervisor(null)}>
                Cancel
              </button>
            </div>
          </section>
        </div>,
        document.body,
      )}
      {pendingArchive && createPortal(
        <div
          className="web-settings-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setPendingArchive(null);
          }}
        >
          <section className="web-archive-modal" role="alertdialog" aria-modal="true" aria-labelledby="web-archive-title" aria-describedby="web-archive-description">
            <div className="web-archive-modal-icon"><Archive size={18} aria-hidden="true" /></div>
            <div className="web-archive-modal-copy">
              <h2 id="web-archive-title">Archive thread?</h2>
              <p id="web-archive-description">
                “{pendingArchive.label}” will leave this list, but its history remains recoverable in Codex.
              </p>
            </div>
            <div className="web-archive-modal-actions">
              <button type="button" onClick={() => setPendingArchive(null)}>Cancel</button>
              <button
                type="button"
                className="is-primary"
                autoFocus
                disabled={busy}
                onClick={() => {
                  onArchiveThread(pendingArchive.workspaceId, pendingArchive.threadId);
                  setPendingArchive(null);
                }}
              >
                Archive
              </button>
            </div>
          </section>
        </div>,
        document.body,
      )}
    </div>
  );
}
