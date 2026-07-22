import { describe, expect, it } from "vitest";
import { parseWebUserInputRequest } from "./webUserInput";

describe("parseWebUserInputRequest", () => {
  it("normalizes the generated requestUserInput contract", () => {
    expect(parseWebUserInputRequest("ws-1", 42, {
      threadId: "thread-1",
      turnId: "turn-1",
      itemId: "item-1",
      autoResolutionMs: 60_000,
      questions: [{
        id: "review_choice",
        header: "Next step",
        question: "How should I continue?",
        isOther: true,
        isSecret: false,
        options: [
          { label: "Fix and review", description: "Apply the fixes first." },
          { label: "Push now", description: "Accept the current risk." },
        ],
      }],
    })).toMatchObject({
      workspace_id: "ws-1",
      request_id: 42,
      params: {
        thread_id: "thread-1",
        turn_id: "turn-1",
        item_id: "item-1",
        auto_resolution_ms: 60_000,
        questions: [{
          id: "review_choice",
          isOther: true,
          isSecret: false,
          options: [{ label: "Fix and review" }, { label: "Push now" }],
        }],
      },
    });
  });
});
