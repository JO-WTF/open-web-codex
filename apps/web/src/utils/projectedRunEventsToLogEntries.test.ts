import { describe, expect, it } from "vitest";
import type { PlatformApproval, PlatformRunEvent } from "../services/platformTypes";
import {
  latestTurnId,
  maxEventSequence,
  projectedEventsToLogEntries,
} from "./projectedRunEventsToLogEntries";

function event(sequence: number, eventType: string, itemId: string, itemType: string, data: Record<string, unknown>): PlatformRunEvent {
  return {
    id: `event-${sequence}`,
    sequence,
    run_id: "run-1",
    event_type: eventType,
    projection_version: 1,
    thread_id: "thread-1",
    turn_id: "turn-1",
    item_id: itemId,
    payload: {
      schemaVersion: 1,
      threadId: "thread-1",
      turnId: "turn-1",
      itemId,
      lifecycle: eventType.split(".").slice(-1)[0] ?? "unknown",
      itemType,
      data,
    },
    created_at: "2026-07-15T00:00:00Z",
  };
}

describe("projectedRunEventsToLogEntries", () => {
  it("maps completed agent messages into assistant log entries", () => {
    const entries = projectedEventsToLogEntries([
      event(1, "codex.item.completed", "agent-1", "agentMessage", {
        type: "agentMessage",
        text: "Done",
      }),
    ]);
    expect(entries).toEqual([
      expect.objectContaining({ level: "assistant", text: "Done" }),
    ]);
  });

  it("merges pending approvals by approval id", () => {
    const approval: PlatformApproval = {
      id: "appr-1",
      run_id: "run-1",
      request_type: "item/commandExecution/requestApproval",
      request_payload: { command: ["echo", "hello"] },
      status: "pending",
      codex_request_id: "42",
      workspace_id: null,
      thread_id: "thread-1",
      decision: null,
      decided_by: null,
      decided_at: null,
      created_at: "2026-07-15T00:00:00Z",
      expires_at: null,
    };
    const entries = projectedEventsToLogEntries([], [approval]);
    expect(entries[0]).toMatchObject({
      kind: "approval",
      approvalId: "appr-1",
      approvalStatus: "pending",
      text: "echo hello",
    });
  });

  it("tracks latest turn id and max sequence", () => {
    const events = [
      event(1, "codex.item.started", "agent-1", "agentMessage", { type: "agentMessage", text: "" }),
      event(2, "codex.item.completed", "agent-1", "agentMessage", { type: "agentMessage", text: "Hi" }),
    ];
    expect(latestTurnId(events)).toBe("turn-1");
    expect(maxEventSequence(events)).toBe(2);
  });
});
