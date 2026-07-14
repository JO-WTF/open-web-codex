import { describe, expect, it } from "vitest";
import type { ProjectedRunEvent } from "../types";
import {
  buildItemsFromProjectedEvents,
  recoverConversationItems,
} from "./projectedThreadEvents";

function event(
  sequence: number,
  eventType: string,
  itemId: string,
  itemType: string | null,
  data: Record<string, unknown>,
): ProjectedRunEvent {
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
    created_at: "2026-07-14T00:00:00Z",
  };
}

describe("projectedThreadEvents", () => {
  it("replays ordered item snapshots and deltas after reconnect", () => {
    const items = buildItemsFromProjectedEvents([
      event(3, "codex.item.completed", "agent-1", "agentMessage", {
        type: "agentMessage",
        text: "Hello world",
      }),
      event(1, "codex.item.started", "agent-1", "agentMessage", {
        type: "agentMessage",
        text: "",
      }),
      event(2, "codex.item.delta", "agent-1", null, {
        sourceType: "item/agentMessage/delta",
        delta: "Hello ",
      }),
    ]);

    expect(items).toEqual([
      {
        id: "agent-1",
        kind: "message",
        role: "assistant",
        text: "Hello world",
      },
    ]);
  });

  it("uses Codex history as authority and keeps projection-only active items", () => {
    const recovered = recoverConversationItems(
      {
        turns: [
          {
            items: [
              { id: "agent-1", type: "agentMessage", text: "Final from Codex" },
            ],
          },
        ],
      },
      [
        event(1, "codex.item.completed", "agent-1", "agentMessage", {
          type: "agentMessage",
          text: "Stale projection",
        }),
        event(2, "codex.item.started", "tool-1", "webSearch", {
          type: "webSearch",
          query: "latest docs",
          status: "inProgress",
        }),
      ],
    );

    expect(recovered).toMatchObject([
      { id: "agent-1", kind: "message", text: "Final from Codex" },
      { id: "tool-1", kind: "tool", status: "inProgress" },
    ]);
  });

  it("keeps unknown completed item types visible after refresh", () => {
    const items = buildItemsFromProjectedEvents([
      event(1, "codex.item.completed", "future-1", "futureItem", {
        type: "futureItem",
        summary: "Unsupported runtime item",
      }),
    ]);

    expect(items).toMatchObject([
      {
        id: "future-1",
        kind: "tool",
        toolType: "futureItem",
        title: "Unsupported item: futureItem",
      },
    ]);
  });
});
