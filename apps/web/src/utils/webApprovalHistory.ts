import type { LogEntry } from "../WebApp";

const STORAGE_KEY = "open-web-codex:approval-history:v1";
const MAX_APPROVALS_PER_THREAD = 100;

type ApprovalHistory = Record<string, Record<string, LogEntry[]>>;

function readHistory(): ApprovalHistory {
  if (typeof window === "undefined") return {};
  try {
    const parsed = JSON.parse(window.localStorage.getItem(STORAGE_KEY) ?? "{}");
    return parsed && typeof parsed === "object" ? parsed as ApprovalHistory : {};
  } catch {
    return {};
  }
}

function approvalKey(entry: LogEntry) {
  return entry.approvalId ?? (entry.approvalRequestId === undefined
    ? entry.id
    : String(entry.approvalRequestId));
}

function writeHistory(history: ApprovalHistory) {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(history));
  } catch {
    // Approval handling must keep working when browser storage is unavailable
    // or full; persistence is a best-effort projection of Codex state.
  }
}

export function saveWebApproval(
  workspaceId: string,
  threadId: string,
  approval: LogEntry,
) {
  if (typeof window === "undefined" || approval.kind !== "approval") return;
  const history = readHistory();
  const entries = history[workspaceId]?.[threadId] ?? [];
  const key = approvalKey(approval);
  const next = [
    ...entries.filter((entry) => approvalKey(entry) !== key),
    approval,
  ].slice(-MAX_APPROVALS_PER_THREAD);
  history[workspaceId] = { ...history[workspaceId], [threadId]: next };
  writeHistory(history);
}

export function resolveStoredWebApproval(
  workspaceId: string,
  requestId: number | string,
  status: "accepted" | "declined" | "resolved",
) {
  if (typeof window === "undefined") return;
  const history = readHistory();
  const workspace = history[workspaceId];
  if (!workspace) return;
  let changed = false;
  const nextWorkspace = Object.fromEntries(Object.entries(workspace).map(([threadId, entries]) => [
    threadId,
    entries.map((entry) => {
      if (entry.approvalRequestId !== requestId) return entry;
      changed = true;
      return { ...entry, approvalStatus: status };
    }),
  ]));
  if (!changed) return;
  history[workspaceId] = nextWorkspace;
  writeHistory(history);
}

export function loadWebApprovalHistory(workspaceId: string, threadId: string): LogEntry[] {
  return (readHistory()[workspaceId]?.[threadId] ?? []).map((entry) => ({
    ...entry,
    // Server request ids are connection-scoped. A restored pending card is
    // historical evidence, not an actionable request.
    approvalStatus: entry.approvalStatus === "pending" ? "resolved" : entry.approvalStatus,
  }));
}
