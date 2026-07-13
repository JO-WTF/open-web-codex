import { useState } from "react";

type McpServerEntry = {
  name: string;
  status: string;
  error?: string | null;
  failureReason?: string | null;
};

type Props = {
  servers: Record<string, McpServerEntry>;
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

export default function McpStatus({ servers }: Props) {
  const [collapsed, setCollapsed] = useState(true);
  const entries = Object.values(servers);
  if (entries.length === 0) return null;

  const errorCount = entries.filter((e) => e.status === "error").length;
  const readyCount = entries.filter((e) => e.status === "ready").length;

  return (
    <div className="web-mcp-panel">
      <div
        className="web-mcp-toggle"
        onClick={() => setCollapsed(!collapsed)}
      >
        <span className="web-mcp-toggle-arrow">{collapsed ? "▶" : "▼"}</span>
        <span className="web-mcp-toggle-label">MCP Servers</span>
        {errorCount > 0 && (
          <span className="web-mcp-badge-error">{errorCount} err</span>
        )}
        <span className="web-mcp-count">
          {readyCount}/{entries.length}
        </span>
      </div>
      {!collapsed && (
        <div className="web-mcp-list">
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
