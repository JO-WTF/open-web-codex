import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { requireCompletedTurn } from "./e2e-turn-contract.mjs";

const completion = (status, error) => ({
  event_type: "codex.turn.completed",
  thread_id: "thread-1",
  turn_id: "turn-1",
  payload: {
    data: {
      turn: {
        id: "turn-1",
        status,
        ...(error ? { error } : {}),
      },
    },
  },
});

describe("requireCompletedTurn", () => {
  it("returns undefined while the authoritative completion is absent", () => {
    assert.equal(
      requireCompletedTurn([], {
        threadId: "thread-1",
        turnId: "turn-1",
        label: "Turn",
      }),
      undefined,
    );
  });

  it("accepts only a typed completed terminal result", () => {
    const events = [completion("completed")];
    assert.equal(
      requireCompletedTurn(events, {
        threadId: "thread-1",
        turnId: "turn-1",
        label: "Turn",
      }),
      events,
    );
  });

  it("preserves a typed provider failure instead of treating turn/completed as success", () => {
    assert.throws(
      () =>
        requireCompletedTurn(
        [completion("failed", { message: "402 Insufficient Balance" })],
        {
          threadId: "thread-1",
          turnId: "turn-1",
          label: "Root Turn",
          sanitize: JSON.stringify,
        },
      ),
      {
        message:
          'Root Turn reached failed: {"message":"402 Insufficient Balance"}',
      },
    );
  });

  it("rejects a completion that omits the current typed status contract", () => {
    const event = completion("completed");
    delete event.payload.data.turn.status;
    assert.throws(
      () =>
        requireCompletedTurn([event], {
          threadId: "thread-1",
          turnId: "turn-1",
          label: "Turn",
        }),
      { message: "Turn completion omitted its typed status" },
    );
  });
});
