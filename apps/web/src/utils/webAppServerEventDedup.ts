import type { AppServerEvent } from "../types";

export function appServerEventDedupKey(event: AppServerEvent): string | null {
  if (
    typeof event.run_id !== "string"
    || event.run_id.length === 0
    || !Number.isSafeInteger(event.sequence)
    || (event.sequence ?? 0) <= 0
  ) {
    return null;
  }
  return `${event.run_id}:${event.sequence}`;
}

export function rememberAppServerEvent(
  lastSequenceByRun: Map<string, number>,
  event: AppServerEvent,
): boolean {
  const key = appServerEventDedupKey(event);
  if (!key) return false;
  const previous = lastSequenceByRun.get(event.run_id as string) ?? 0;
  if ((event.sequence as number) <= previous) return false;
  lastSequenceByRun.set(event.run_id as string, event.sequence as number);
  return true;
}
