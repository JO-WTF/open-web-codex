import { describe, expect, it } from "vitest";
import type { MessageEntry } from "./MessageList";
import { initialConversationStart, previousConversationStart } from "./conversationWindow";

function turns(count: number): MessageEntry[] {
  return Array.from({ length: count }, (_, index) => [
    { id: `user-${index}`, level: "user" as const, text: `Question ${index}` },
    { id: `assistant-${index}`, level: "assistant" as const, text: `Answer ${index}` },
  ]).flat();
}

describe("conversation window", () => {
  it("starts with the latest ten user turns and loads ten older turns at a time", () => {
    const items = turns(25);
    const initial = initialConversationStart(items);
    expect(initial).toBe(30);
    expect(previousConversationStart(items, initial)).toBe(10);
    expect(previousConversationStart(items, 10)).toBe(0);
  });
});
