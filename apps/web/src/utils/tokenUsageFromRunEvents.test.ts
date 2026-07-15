import { describe, expect, it } from "vitest";
import { tokenUsageFromRunEvents } from "./tokenUsageFromRunEvents";

describe("tokenUsageFromRunEvents", () => {
  it("reads the latest projected token usage event", () => {
    const usage = tokenUsageFromRunEvents([
      {
        id: "1",
        sequence: 1,
        run_id: "run-1",
        event_type: "codex.thread.token_usage",
        projection_version: 1,
        thread_id: "thread-1",
        turn_id: null,
        item_id: null,
        created_at: "2026-01-01T00:00:00Z",
        payload: {
          schemaVersion: 1,
          threadId: "thread-1",
          lifecycle: "updated",
          data: {
            tokenUsage: {
              total: { totalTokens: 10, inputTokens: 8, cachedInputTokens: 0, outputTokens: 2, reasoningOutputTokens: 0 },
              last: { totalTokens: 4, inputTokens: 3, cachedInputTokens: 0, outputTokens: 1, reasoningOutputTokens: 0 },
            },
            modelContextWindow: 128000,
          },
        },
      },
    ]);
    expect(usage?.modelContextWindow).toBe(128000);
    expect(usage?.last.totalTokens).toBe(4);
  });
});
