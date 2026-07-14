import { describe, expect, it } from "vitest";
import type { AppServerEvent } from "../types";
import { rememberAppServerEvent } from "./webAppServerEventDedup";

const event = (method: string): AppServerEvent => ({
  workspace_id: "workspace-1",
  message: { method, params: { itemId: "item-1", status: "completed" } },
});

describe("rememberAppServerEvent", () => {
  it("drops a repeated lifecycle event within the replay window", () => {
    const keys = new Map<string, number>();
    expect(rememberAppServerEvent(keys, event("item/completed"), 1_000)).toBe(true);
    expect(rememberAppServerEvent(keys, event("item/completed"), 2_000)).toBe(false);
  });

  it("never deduplicates streaming deltas", () => {
    const keys = new Map<string, number>();
    expect(rememberAppServerEvent(keys, event("item/agentMessage/delta"), 1_000)).toBe(true);
    expect(rememberAppServerEvent(keys, event("item/agentMessage/delta"), 1_001)).toBe(true);
  });

  it("accepts the same notification after the replay window", () => {
    const keys = new Map<string, number>();
    expect(rememberAppServerEvent(keys, event("serverRequest/resolved"), 1_000)).toBe(true);
    expect(rememberAppServerEvent(keys, event("serverRequest/resolved"), 32_000)).toBe(true);
  });
});
