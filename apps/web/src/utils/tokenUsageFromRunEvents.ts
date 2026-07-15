import type { PlatformRunEvent } from "../services/platformTypes";
import type { ThreadTokenUsage } from "../types";
import { normalizeTokenUsage } from "../features/threads/utils/threadNormalize";

export function tokenUsageFromRunEvents(events: PlatformRunEvent[]): ThreadTokenUsage | null {
  for (let index = events.length - 1; index >= 0; index -= 1) {
    const event = events[index];
    if (event.event_type !== "codex.thread.token_usage") {
      continue;
    }
    const data = event.payload?.data;
    if (!data || typeof data !== "object") {
      continue;
    }
    const record = data as Record<string, unknown>;
    const tokenUsage =
      (record.tokenUsage as Record<string, unknown> | undefined)
      ?? (record.token_usage as Record<string, unknown> | undefined);
    if (!tokenUsage) {
      continue;
    }
    const modelContextWindow =
      record.modelContextWindow
      ?? record.model_context_window
      ?? tokenUsage.modelContextWindow
      ?? tokenUsage.model_context_window;
    return normalizeTokenUsage({
      ...tokenUsage,
      modelContextWindow,
    });
  }
  return null;
}
