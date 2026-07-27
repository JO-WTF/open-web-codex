import Bot from "lucide-react/dist/esm/icons/bot";
import ChevronRight from "lucide-react/dist/esm/icons/chevron-right";
import FolderOpen from "lucide-react/dist/esm/icons/folder-open";
import PanelLeftClose from "lucide-react/dist/esm/icons/panel-left-close";
import PanelLeftOpen from "lucide-react/dist/esm/icons/panel-left-open";

type Props = {
  workspaceName: string | null;
  threadTitle: string | null;
  threadStatus?: string;
  threadSettings?: Record<string, unknown> | null;
  sidebarCollapsed: boolean;
  onToggleSidebar: () => void;
  rightPanelOpen?: boolean;
  activeRightPanelTab?: "agents" | "files";
  agentPanelAvailable?: boolean;
  agentPanelUnread?: boolean;
  onOpenAgentPanel?: () => void;
  onOpenFilePanel?: () => void;
};

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
  threadStatus,
  threadSettings,
  sidebarCollapsed,
  onToggleSidebar,
  rightPanelOpen = false,
  activeRightPanelTab = "files",
  agentPanelAvailable = true,
  agentPanelUnread = false,
  onOpenAgentPanel,
  onOpenFilePanel,
}: Props) {
  // Extract useful settings for display
  const ts = threadSettings as Record<string, unknown> | null | undefined;
  const rawSettings = ts?.threadSettings as Record<string, unknown> | null | undefined;
  const collabMode = rawSettings?.collaborationMode as Record<string, unknown> | null | undefined;
  const collabLabel = typeof collabMode?.mode === "string" ? collabMode.mode : null;
  const approvalPolicy = typeof rawSettings?.approvalPolicy === "string" ? rawSettings.approvalPolicy : null;
  return (
    <div className="web-chat-header">
      <div className="web-chat-header-left">
        <button
          type="button"
          className="web-icon-button"
          title={sidebarCollapsed ? "Expand projects panel" : "Collapse projects panel"}
          aria-label={sidebarCollapsed ? "Expand projects panel" : "Collapse projects panel"}
          onClick={onToggleSidebar}
        >
          {sidebarCollapsed ? <PanelLeftOpen size={16} /> : <PanelLeftClose size={16} />}
        </button>
        <span className="web-chat-workspace">{workspaceName ?? "Select a project"}</span>
        {threadTitle ? (
          <>
            <ChevronRight size={13} className="web-chat-header-sep" aria-hidden="true" />
            <span className="web-chat-title">{threadTitle}</span>
          </>
        ) : null}
        {threadStatus && threadStatus !== "idle" && (
          <span className={`web-status-dot ${statusDotClass(threadStatus)}`} title={`Thread: ${threadStatus}`} />
        )}
      </div>
      <div className="web-chat-header-right">
        {threadStatus && threadStatus.includes("waitingOnApproval") && (
          <span className="web-header-approval-badge" title="Waiting for approval">
            ⚠ Approval needed
          </span>
        )}
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
        <button
          type="button"
          className={`web-icon-button web-agent-panel-button${rightPanelOpen && activeRightPanelTab === "agents" ? " is-active" : ""}`}
          title="Agent activity"
          aria-label="Agent activity"
          aria-pressed={rightPanelOpen && activeRightPanelTab === "agents"}
          disabled={!agentPanelAvailable}
          onClick={onOpenAgentPanel}
        >
          <Bot size={16} />
          {agentPanelUnread ? <span className="web-agent-unread" aria-label="New Agent activity" /> : null}
        </button>
        <button type="button" className={`web-icon-button${rightPanelOpen && activeRightPanelTab === "files" ? " is-active" : ""}`} title="File manager" aria-label="File manager" aria-pressed={rightPanelOpen && activeRightPanelTab === "files"} onClick={onOpenFilePanel}>
          <FolderOpen size={16} />
        </button>
      </div>
    </div>
  );
}
