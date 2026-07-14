import type { AppServerEvent } from "../types";

export function summarizeWebAppServerEvent(event: AppServerEvent): string | null {
  const message = event.message ?? {};
  const method = typeof message.method === "string" ? message.method : null;
  if (!method) return null;

  const params =
    message.params && typeof message.params === "object"
      ? (message.params as Record<string, unknown>)
      : {};

  if (method === "codex/connected") {
    return "Workspace connected";
  }

  if (method === "remoteControl/status/changed") {
    const status = typeof params.status === "string" ? params.status : "unknown";
    const serverName = typeof params.serverName === "string" ? params.serverName.trim() : "";
    const statusLabel = status === "enabled"
      ? "enabled"
      : status === "disabled"
        ? "disabled"
        : status;
    return `Remote control ${statusLabel}${serverName ? ` · ${serverName}` : ""}`;
  }

  const threadId =
    typeof params.threadId === "string"
      ? params.threadId
      : typeof params.thread_id === "string"
        ? params.thread_id
        : null;
  const item = params.item && typeof params.item === "object" ? params.item : null;
  const detail = item
    ? JSON.stringify(item).slice(0, 240)
    : JSON.stringify(params).slice(0, 240);
  return `${method}${threadId ? ` · ${threadId}` : ""}${detail && detail !== "{}" ? ` · ${detail}` : ""}`;
}
