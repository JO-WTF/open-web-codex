import type { PlatformRunEvent } from "../services/platformTypes";
import type { MessageEntry } from "../components/Conversation/MessageList";

export function appendRunEvents(
  existing: PlatformRunEvent[],
  incoming: PlatformRunEvent[],
): PlatformRunEvent[] {
  if (incoming.length === 0) {
    return existing;
  }
  const seen = new Set(existing.map((event) => event.sequence));
  const merged = [...existing];
  for (const event of incoming) {
    if (seen.has(event.sequence)) {
      continue;
    }
    seen.add(event.sequence);
    merged.push(event);
  }
  merged.sort((left, right) => left.sequence - right.sequence);
  return merged;
}

export function shouldPollTaskRun(status: string | null | undefined) {
  return status === "running"
    || status === "provisioning"
    || status === "queued"
    || status === "waiting_approval"
    || status === "pending";
}

export function runStartIdempotencyKey(taskId: string) {
  return `active-run:${taskId}`;
}

export function selectedEffortStorageKey(taskId: string) {
  return `open-web-codex:task-effort:${taskId}`;
}

export function readStoredEffort(taskId: string | null): string | null {
  if (!taskId) {
    return null;
  }
  try {
    return localStorage.getItem(selectedEffortStorageKey(taskId));
  } catch {
    return null;
  }
}

export function writeStoredEffort(taskId: string, effort: string) {
  try {
    localStorage.setItem(selectedEffortStorageKey(taskId), effort);
  } catch {
    // ignore storage failures
  }
}

export { mergeProjectedMessages };

function mergeProjectedMessages(projected: MessageEntry[], pendingUser: MessageEntry[]) {
  if (pendingUser.length === 0) {
    return projected;
  }
  const seen = new Set(
    projected
      .filter((entry) => entry.level === "user")
      .map((entry) => entry.text.trim()),
  );
  const extras = pendingUser.filter((entry) => !seen.has(entry.text.trim()));
  return [...projected, ...extras];
}
