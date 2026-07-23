export type WebMcpServer = {
  name: string;
  status: string;
  error?: string | null;
  failureReason?: string | null;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function unwrapResult(value: unknown): Record<string, unknown> {
  let current = value;
  while (isRecord(current) && isRecord(current.result)) current = current.result;
  return isRecord(current) ? current : {};
}

function unwrapDataArray(value: unknown): Array<Record<string, unknown>> {
  const payload = unwrapResult(value);
  return Array.isArray(payload.data)
    ? payload.data.filter(isRecord)
    : [];
}

export function formatMcpStatusLines(value: unknown): string[] {
  const data = unwrapDataArray(value);
  if (data.length === 0) return ["可用 MCP：", "- 暂无已配置的 MCP 服务。"];

  const lines = ["可用 MCP："];
  for (const server of [...data].sort((a, b) => String(a.name ?? "").localeCompare(String(b.name ?? "")))) {
    const name = typeof server.name === "string" && server.name.trim()
      ? server.name.trim()
      : "unknown";
    const status = typeof server.status === "string" && server.status.trim()
      ? server.status.trim()
      : isRecord(server.serverInfo) || (isRecord(server.tools) && Object.keys(server.tools).length > 0)
        ? "ready"
        : "unavailable";
    lines.push(`- ${name}（${status}）`);

    const tools = isRecord(server.tools) ? Object.keys(server.tools) : [];
    const prefix = `mcp__${name}__`;
    const toolNames = tools
      .map((tool) => tool.startsWith(prefix) ? tool.slice(prefix.length) : tool)
      .filter(Boolean)
      .sort((a, b) => a.localeCompare(b));
    if (toolNames.length > 0) {
      lines.push(`  工具：${toolNames.join("、")}`);
    }

    const failure = typeof server.failureReason === "string" && server.failureReason.trim()
      ? server.failureReason.trim()
      : typeof server.error === "string" && server.error.trim()
        ? server.error.trim()
        : "";
    if (failure) lines.push(`  状态说明：${failure}`);
  }
  return lines;
}

export function parseInitialMcpServers(value: unknown): Record<string, WebMcpServer> {
  const data = unwrapDataArray(value);
  const servers: Record<string, WebMcpServer> = {};

  for (const candidate of data) {
    if (!isRecord(candidate) || typeof candidate.name !== "string" || !candidate.name) continue;
    const hasInventory = isRecord(candidate.serverInfo)
      || (isRecord(candidate.tools) && Object.keys(candidate.tools).length > 0)
      || (Array.isArray(candidate.resources) && candidate.resources.length > 0)
      || (Array.isArray(candidate.resourceTemplates) && candidate.resourceTemplates.length > 0);
    servers[candidate.name] = {
      name: candidate.name,
      status: hasInventory ? "ready" : "unavailable",
      failureReason: candidate.authStatus === "notLoggedIn" ? "Authentication required" : null,
    };
  }

  return servers;
}

export function parseInitialRateLimits(value: unknown): Record<string, unknown> | null {
  const payload = unwrapResult(value);
  return isRecord(payload.rateLimits) ? payload.rateLimits : null;
}

export function mergeRateLimits(
  base: Record<string, unknown> | null,
  update: Record<string, unknown> | null,
): Record<string, unknown> | null {
  if (!base) return update;
  if (!update) return base;
  const merged = { ...base, ...update };
  for (const key of ["primary", "secondary", "credits"] as const) {
    if (isRecord(base[key]) && isRecord(update[key])) {
      merged[key] = { ...base[key], ...update[key] };
    }
  }
  return merged;
}
