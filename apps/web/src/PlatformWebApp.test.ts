import { describe, expect, it } from "vitest";
import { isRunInFlight, isRunTerminal, mergeProjectedMessages } from "./PlatformWebApp";

describe("PlatformWebApp run helpers", () => {
  it("treats waiting_approval as in-flight", () => {
    expect(isRunInFlight("waiting_approval")).toBe(true);
    expect(isRunTerminal("waiting_approval")).toBe(false);
  });

  it("merges pending user messages until projection catches up", () => {
    const projected = [{ id: "a1", level: "assistant" as const, text: "Hi" }];
    const pending = [{ id: "u1", level: "user" as const, text: "hello" }];
    expect(mergeProjectedMessages(projected, pending)).toEqual([
      ...projected,
      ...pending,
    ]);
  });

  it("drops pending user messages once projection includes them", () => {
    const projected = [{ id: "u1", level: "user" as const, text: "hello" }];
    const pending = [{ id: "u-pending", level: "user" as const, text: "hello" }];
    expect(mergeProjectedMessages(projected, pending)).toEqual(projected);
  });
});
