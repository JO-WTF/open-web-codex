import type { LogEntry } from "../types/logEntry";
import type { ConversationItem } from "../types";
import type { PlatformApproval, PlatformRunEvent } from "../services/platformTypes";
import { buildItemsFromProjectedEvents } from "./projectedThreadEvents";
import { diffLines, webLogEntryFromThreadItem } from "./webThreadHistory";

function commandText(value: unknown) {
  return Array.isArray(value)
    ? value.map((part) => String(part)).join(" ")
    : typeof value === "string"
      ? value
      : "";
}

function mapApprovalToLogEntry(approval: PlatformApproval): LogEntry {
  const payload = approval.request_payload ?? {};
  const command = commandText(payload.command) || approval.request_type;
  const status =
    approval.status === "approved"
      ? "accepted"
      : approval.status === "rejected"
        ? "declined"
        : approval.status === "pending"
          ? "pending"
          : "resolved";
  return {
    id: `approval-${approval.id}`,
    level: "info",
    text: command,
    kind: "approval",
    approvalId: approval.id,
    approvalRequestId: approval.codex_request_id ?? undefined,
    approvalStatus: status,
  };
}

function conversationItemToLogEntry(item: ConversationItem): LogEntry | null {
  if (item.kind === "message") {
    return {
      id: item.id,
      level: item.role === "user" ? "user" : "assistant",
      text: item.text,
    };
  }
  if (item.kind === "reasoning") {
    return webLogEntryFromThreadItem(
      {
        id: item.id,
        type: "reasoning",
        summary: item.summary,
        content: item.content,
      },
      () => item.id,
    );
  }
  if (item.kind === "diff") {
    return {
      id: item.id,
      level: "info",
      text: item.title,
      kind: "diff",
      diffTitle: item.title,
      diffLines: diffLines(item.diff),
      streaming: item.status === "inProgress",
    };
  }
  if (item.kind === "tool" && item.toolType === "commandExecution") {
    return {
      id: item.id,
      level: "info",
      text: item.title.replace(/^Command:\s*/, ""),
      kind: "command_exec",
      toolStatus: item.status,
      cmdOutput: item.output,
      cmdDurationMs: item.durationMs ?? undefined,
      cmdCwd: item.detail || undefined,
    };
  }
  if (item.kind === "tool") {
    return {
      id: item.id,
      level: "info",
      text: item.title,
      kind: "tool",
      toolType: item.toolType,
      toolTitle: item.title,
      toolStatus: item.status,
      toolDetail: item.detail,
      toolOutput: item.output,
    };
  }
  return webLogEntryFromThreadItem({ id: item.id, type: item.kind, text: item.kind }, () => item.id);
}

export function projectedEventsToLogEntries(
  events: PlatformRunEvent[],
  approvals: PlatformApproval[] = [],
): LogEntry[] {
  const projected = buildItemsFromProjectedEvents(
    events.map((event) => ({
      ...event,
      thread_id: event.thread_id,
    })),
  );

  const fromItems = projected.flatMap((item) => {
    const entry = conversationItemToLogEntry(item);
    return entry ? [entry] : [];
  });

  const approvalEntries = approvals.map(mapApprovalToLogEntry);
  const merged = [...fromItems];
  for (const approvalEntry of approvalEntries) {
    const existing = merged.findIndex(
      (entry) => entry.approvalId && entry.approvalId === approvalEntry.approvalId,
    );
    if (existing >= 0) {
      merged[existing] = { ...merged[existing], ...approvalEntry, id: merged[existing].id };
    } else {
      merged.push(approvalEntry);
    }
  }
  return merged;
}

export function latestTurnId(events: PlatformRunEvent[]): string | null {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const turnId = events[index]?.turn_id ?? events[index]?.payload.turnId;
    if (typeof turnId === "string" && turnId.trim()) {
      return turnId;
    }
  }
  return null;
}

export function maxEventSequence(events: PlatformRunEvent[]): number {
  return events.reduce((max, event) => Math.max(max, event.sequence), 0);
}

export function turnStartedAtFromEvents(events: PlatformRunEvent[]): number | null {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index];
    if (event.event_type !== "codex.turn.started") {
      continue;
    }
    const startedAtMs = event.payload?.data?.startedAtMs;
    if (typeof startedAtMs === "number" && Number.isFinite(startedAtMs)) {
      return startedAtMs;
    }
    const createdAt = Date.parse(event.created_at);
    return Number.isFinite(createdAt) ? createdAt : Date.now();
  }
  return null;
}
