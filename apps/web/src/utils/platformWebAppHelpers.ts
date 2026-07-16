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

export function activeProjectStorageKey() {
  return "open-web-codex:active-project-id";
}

export function activeTaskStorageKey(projectId: string) {
  return `open-web-codex:active-task-id:${projectId}`;
}

export function readStoredActiveProjectId(): string | null {
  try {
    return localStorage.getItem(activeProjectStorageKey());
  } catch {
    return null;
  }
}

export function writeStoredActiveProjectId(projectId: string | null) {
  try {
    const key = activeProjectStorageKey();
    if (!projectId) {
      localStorage.removeItem(key);
      return;
    }
    localStorage.setItem(key, projectId);
  } catch {
    // ignore storage failures
  }
}

export function readStoredActiveTaskId(projectId: string | null): string | null {
  if (!projectId) {
    return null;
  }
  try {
    return localStorage.getItem(activeTaskStorageKey(projectId));
  } catch {
    return null;
  }
}

export function writeStoredActiveTaskId(projectId: string | null, taskId: string | null) {
  if (!projectId) {
    return;
  }
  try {
    const key = activeTaskStorageKey(projectId);
    if (!taskId) {
      localStorage.removeItem(key);
      return;
    }
    localStorage.setItem(key, taskId);
  } catch {
    // ignore storage failures
  }
}

export function resolveActiveTaskId(
  items: Array<{ id: string }>,
  preferredId: string | null | undefined,
): string | null {
  if (preferredId && items.some((item) => item.id === preferredId)) {
    return preferredId;
  }
  return items[0]?.id ?? null;
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
