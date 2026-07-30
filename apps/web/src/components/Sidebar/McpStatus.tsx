import { useEffect, useId, useState } from "react";

type McpServerEntry = {
  name: string;
  status: string;
  error?: string | null;
  failureReason?: string | null;
};

type Props = {
  servers: Record<string, McpServerEntry>;
  expandRequest?: number;
};

function statusIcon(status: string): string {
  switch (status) {
    case "ready":
      return "●";
    case "starting":
      return "◌";
    case "error":
      return "✕";
    default:
      return "○";
  }
}

function statusClass(status: string): string {
  switch (status) {
    case "ready":
      return "mcp-status-ready";
    case "starting":
      return "mcp-status-starting";
    case "error":
      return "mcp-status-error";
    default:
      return "mcp-status-other";
  }
}

export default function McpStatus({ servers, expandRequest = 0 }: Props) {
  const [collapsed, setCollapsed] = useState(true);
  const listId = useId();
  useEffect(() => {
    if (expandRequest > 0) setCollapsed(false);
  }, [expandRequest]);
  const entries = Object.values(servers);
  if (entries.length === 0) return null;

  const errorCount = entries.filter((e) => e.status === "error").length;
  const readyCount = entries.filter((e) => e.status === "ready").length;

  return (
    <div className="web-mcp-panel">
      <button
        type="button"
        className="web-mcp-toggle"
        aria-expanded={!collapsed}
        aria-controls={listId}
        onClick={() => setCollapsed((current) => !current)}
      >
        <span className="web-mcp-toggle-arrow" aria-hidden="true">
          {collapsed ? "▶" : "▼"}
        </span>
        <span className="web-mcp-toggle-label">MCP Servers</span>
        {errorCount > 0 && (
          <span className="web-mcp-badge-error">{errorCount} err</span>
        )}
        <span className="web-mcp-count">
          {readyCount}/{entries.length}
        </span>
      </button>
      {!collapsed && (
        <div className="web-mcp-list" id={listId}>
          {entries.map((srv) => (
            <div key={srv.name} className="web-mcp-item">
              <span className={`web-mcp-dot ${statusClass(srv.status)}`}>
                {statusIcon(srv.status)}
              </span>
              <span className="web-mcp-name">{srv.name}</span>
              <span className={`web-mcp-status ${statusClass(srv.status)}`}>
                {srv.status}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
