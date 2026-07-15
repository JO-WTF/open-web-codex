import type { LogEntry } from "../types/logEntry";
import type { PlatformApproval, PlatformRunEvent } from "../services/platformTypes";
import { buildItemsFromProjectedEvents } from "./projectedThreadEvents";
import { webLogEntryFromThreadItem } from "./webThreadHistory";

const newLogId = () =>
  crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;

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
    const raw = {
      id: item.id,
      type:
        item.kind === "message"
          ? item.role === "user"
            ? "userMessage"
            : "agentMessage"
          : item.kind === "reasoning"
            ? "reasoning"
            : item.kind,
      ...(item.kind === "message"
        ? { text: item.text }
        : item.kind === "reasoning"
          ? { summary: item.summary, content: item.content }
          : item.kind === "tool"
            ? {
                status: item.status,
                aggregatedOutput: item.output,
                tool: item.title,
              }
            : {}),
    };
    const entry = webLogEntryFromThreadItem(raw, newLogId);
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
