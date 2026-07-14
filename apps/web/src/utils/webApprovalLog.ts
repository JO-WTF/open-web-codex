import type { LogEntry } from "../WebApp";

function isSameApproval(left: LogEntry, right: LogEntry) {
  if (left.kind !== "approval" || right.kind !== "approval") {
    return false;
  }
  if (left.approvalId && right.approvalId) {
    return left.approvalId === right.approvalId;
  }
  return (
    left.approvalRequestId !== undefined &&
    right.approvalRequestId !== undefined &&
    left.approvalRequestId === right.approvalRequestId
  );
}

export function appendWebLogEntry(entries: LogEntry[], next: LogEntry) {
  if (next.kind === "approval") {
    const existingIndex = entries.findIndex((entry) => isSameApproval(entry, next));
    if (existingIndex >= 0) {
      return entries.map((entry, index) => {
        if (index !== existingIndex) {
          return entry;
        }
        const preserveResolution =
          entry.approvalStatus !== undefined && entry.approvalStatus !== "pending";
        return {
          ...entry,
          ...next,
          id: entry.id,
          approvalStatus: preserveResolution
            ? entry.approvalStatus
            : next.approvalStatus,
        };
      });
    }
  }

  return [...entries.slice(-199), next];
}
