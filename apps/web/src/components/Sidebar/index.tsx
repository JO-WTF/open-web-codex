import { useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import Settings from "lucide-react/dist/esm/icons/settings";
import Bot from "lucide-react/dist/esm/icons/bot";
import BookOpen from "lucide-react/dist/esm/icons/book-open";
import Moon from "lucide-react/dist/esm/icons/moon";
import Sun from "lucide-react/dist/esm/icons/sun";
import X from "lucide-react/dist/esm/icons/x";
import type {
  AgentDefinitionSummary,
  RunReadiness,
  RunReadinessAction,
  SupervisorPolicySummary,
} from "../../../browser/types";
import type { WorkspaceInfo } from "../../types";
import Brand from "./Brand";
import Workspaces from "./Workspaces";
import McpStatus from "./McpStatus";
import RateLimitCard from "./RateLimitCard";
import AgentStudioDialog from "./AgentStudioDialog";
import type { RunLaunchSelection } from "./RunLauncherDialog";
import MapsConfigurationModal from "../Conversation/messages/MapsConfigurationModal";
import LearnDialog from "./LearnDialog";

type ThreadInfo = {
  id: string;
  label: string;
  updatedAt: number;
  turnCount?: number;
  status?: string;
  creationStatus?: "creating" | "failed";
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
  supervisorPolicies?: SupervisorPolicySummary[];
  supervisorPoliciesLoading?: boolean;
  supervisorPoliciesError?: string | null;
  agents?: AgentDefinitionSummary[];
  agentsLoading?: boolean;
  agentsError?: string | null;
  onEvaluateReadiness: (
    workspaceId: string,
    selection: RunLaunchSelection,
  ) => Promise<RunReadiness>;
  onStartTask: (
    workspaceId: string,
    selection: RunLaunchSelection,
    readiness: RunReadiness,
    operationId: string,
  ) => Promise<boolean>;
  onReadinessAction: (action: RunReadinessAction, workspaceId: string) => void;
  onArchiveThread: (workspaceId: string, threadId: string) => void;
  onRemoveWorkspace: (workspaceId: string) => void;
  baseUrl: string;
  token: string;
  onBaseUrlChange: (url: string) => void;
  onTokenChange: (token: string) => void;
  onCheckGateway: () => void;
  mcpServers: Record<string, {name: string; status: string; error?: string | null; failureReason?: string | null}>;
  rateLimits: Record<string, unknown> | null;
  currentProviderId: string | null;
  busy: boolean;
  theme: "light" | "dark";
  onToggleTheme: () => void;
  onSupervisorCatalogChanged: () => void;
  onTutorialPromptReady: (
    prompt: string,
    options?: { replaceExisting?: boolean },
  ) => boolean;
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
  supervisorPolicies = [],
  supervisorPoliciesLoading = false,
  supervisorPoliciesError = null,
  agents = [],
  agentsLoading = false,
  agentsError = null,
  onEvaluateReadiness,
  onStartTask,
  onReadinessAction,
  onArchiveThread,
  onRemoveWorkspace,
  baseUrl,
  token,
  onBaseUrlChange,
  onTokenChange,
  onCheckGateway,
  mcpServers,
  rateLimits,
  currentProviderId,
  busy,
  theme,
  onToggleTheme,
  onSupervisorCatalogChanged,
  onTutorialPromptReady,
}: Props) {
  const [showSettings, setShowSettings] = useState(false);
  const [showAgentStudio, setShowAgentStudio] = useState(false);
  const [showMapsSettings, setShowMapsSettings] = useState(false);
  const [showLearn, setShowLearn] = useState(false);
  const [mcpExpandRequest, setMcpExpandRequest] = useState(0);
  const learnTrigger = useRef<HTMLButtonElement | null>(null);

  const closeLearn = useCallback(() => {
    setShowLearn(false);
    window.setTimeout(() => learnTrigger.current?.focus(), 0);
  }, []);

  useEffect(() => {
    if (!showSettings && !showAgentStudio && !showMapsSettings && !showLearn) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setShowSettings(false);
      setShowAgentStudio(false);
      setShowMapsSettings(false);
      closeLearn();
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [closeLearn, showAgentStudio, showLearn, showMapsSettings, showSettings]);

  return (
    <aside className="web-sidebar">
      <div className="web-sidebar-scroll">
        <Brand state={gatewayState} version={gatewayVersion} />

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
          supervisorPolicies={supervisorPolicies}
          supervisorPoliciesLoading={supervisorPoliciesLoading}
          supervisorPoliciesError={supervisorPoliciesError}
          agents={agents}
          agentsLoading={agentsLoading}
          agentsError={agentsError}
          onEvaluateReadiness={onEvaluateReadiness}
          onStartTask={onStartTask}
          onReadinessAction={(action, workspaceId) => {
            if (action === "open_agent_studio") {
              setShowAgentStudio(true);
              return;
            }
            if (action === "open_mcp_status") {
              setMcpExpandRequest((request) => request + 1);
              return;
            }
            if (action === "open_maps_settings") {
              setShowMapsSettings(true);
              return;
            }
            onReadinessAction(action, workspaceId);
          }}
          onArchiveThread={onArchiveThread}
          onRemoveWorkspace={onRemoveWorkspace}
          busy={busy}
        />

        <McpStatus servers={mcpServers} expandRequest={mcpExpandRequest} />


      </div>

      <div className="web-sidebar-bottom">
        {rateLimits && currentProviderId === "openai" ? <RateLimitCard rateLimits={rateLimits} /> : null}
        <div className="web-sidebar-actions">
          <button
            type="button"
            className="web-learn-toggle"
            aria-label="Open Learn"
            aria-haspopup="dialog"
            aria-expanded={showLearn}
            title="Learn"
            onClick={(event) => {
              learnTrigger.current = event.currentTarget;
              setShowLearn(true);
            }}
          >
            <BookOpen size={16} aria-hidden="true" />
            <span>Learn</span>
          </button>
          <button
            type="button"
            className="web-agent-studio-toggle"
            aria-label="Open Agent Studio"
            aria-haspopup="dialog"
            aria-expanded={showAgentStudio}
            title="Agent Studio"
            onClick={() => setShowAgentStudio(true)}
          >
            <Bot size={16} aria-hidden="true" />
            <span>Agent Studio</span>
          </button>
          <button
            type="button"
            className="web-theme-toggle"
            aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
            title={theme === "dark" ? "Light theme" : "Dark theme"}
            onClick={onToggleTheme}
          >
            {theme === "dark" ? <Sun size={16} aria-hidden="true" /> : <Moon size={16} aria-hidden="true" />}
          </button>
          <button
            type="button"
            className="web-settings-toggle"
            aria-label="Open settings"
            aria-haspopup="dialog"
            aria-expanded={showSettings}
            onClick={() => setShowSettings(true)}
          >
            <span className={`web-settings-toggle-icon${showSettings ? " web-settings-icon-open" : ""}`}>
              <Settings size={16} />
            </span>
            Settings
          </button>
        </div>
      </div>
      {showSettings && createPortal(
        <div
          className="web-settings-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setShowSettings(false);
          }}
        >
          <section className="web-settings-modal" role="dialog" aria-modal="true" aria-labelledby="web-settings-title">
            <div className="web-settings-modal-header">
              <div>
                <h2 id="web-settings-title">Settings</h2>
                <p>Configure the Web gateway connection.</p>
              </div>
              <button type="button" className="web-settings-close" aria-label="Close settings" onClick={() => setShowSettings(false)}>
                <X size={17} />
              </button>
            </div>
            <div className="web-settings-panel">
              <label>
                Gateway URL
                <input
                  autoFocus
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
                  type="button"
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
          </section>
        </div>,
        document.body,
      )}
      {showAgentStudio && createPortal(
        <div
          className="web-settings-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) setShowAgentStudio(false);
          }}
        >
          <AgentStudioDialog
            mcpServers={mcpServers}
            workspaces={workspaces}
            activeWorkspaceId={activeWorkspaceId}
            onClose={() => setShowAgentStudio(false)}
            onStartThread={(workspaceId) => {
              setShowAgentStudio(false);
              onNewThread(workspaceId);
            }}
            onSupervisorCatalogChanged={onSupervisorCatalogChanged}
          />
        </div>,
        document.body,
      )}
      {showLearn && createPortal(
        <div
          className="web-settings-backdrop"
          onMouseDown={(event) => {
            if (event.target === event.currentTarget) closeLearn();
          }}
        >
          <LearnDialog
            workspaces={workspaces}
            activeWorkspaceId={activeWorkspaceId}
            busy={busy}
            onEvaluateReadiness={onEvaluateReadiness}
            onStartTask={onStartTask}
            onReadinessAction={(action, workspaceId) => {
              if (action === "open_agent_studio") {
                closeLearn();
                setShowAgentStudio(true);
                return;
              }
              if (action === "open_mcp_status") {
                closeLearn();
                setMcpExpandRequest((request) => request + 1);
                return;
              }
              if (action === "open_maps_settings") {
                closeLearn();
                setShowMapsSettings(true);
                return;
              }
              closeLearn();
              onReadinessAction(action, workspaceId);
            }}
            onCatalogChanged={onSupervisorCatalogChanged}
            onPromptReady={onTutorialPromptReady}
            onClose={closeLearn}
          />
        </div>,
        document.body,
      )}
      {showMapsSettings ? (
        <MapsConfigurationModal
          initialProvider="mapbox"
          onClose={() => setShowMapsSettings(false)}
          onSaved={() => setShowMapsSettings(false)}
        />
      ) : null}
    </aside>
  );
}
