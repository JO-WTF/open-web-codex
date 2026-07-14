import type { AppServerEvent } from "../types";

const STREAMING_METHOD_PARTS = ["/delta", "Delta", "/progress"];

export function appServerEventDedupKey(event: AppServerEvent): string | null {
  const message = event.message ?? {};
  const method = typeof message.method === "string" ? message.method : null;
  if (!method || STREAMING_METHOD_PARTS.some((part) => method.includes(part))) {
    return null;
  }

  // The transitional SSE bridge has no cursor or event id. Use the complete
  // non-streaming notification as a compatibility key until the durable event
  // contract supplies a server-assigned sequence.
  return `${event.workspace_id}:${JSON.stringify(message)}`;
}

export function rememberAppServerEvent(
  recentKeys: Map<string, number>,
  event: AppServerEvent,
  now = Date.now(),
  windowMs = 30_000,
  maxKeys = 1_000,
): boolean {
  const key = appServerEventDedupKey(event);
  if (!key) return true;

  const previous = recentKeys.get(key);
  if (previous !== undefined && now - previous <= windowMs) return false;
  recentKeys.delete(key);
  recentKeys.set(key, now);

  while (recentKeys.size > maxKeys) {
    const oldest = recentKeys.keys().next().value;
    if (oldest === undefined) break;
    recentKeys.delete(oldest);
  }
  return true;
}
