import { describe, expect, it } from "vitest";
import type { AppServerEvent } from "../types";
import { rememberAppServerEvent } from "./webAppServerEventDedup";

const event = (sequence: number, method = "item/completed"): AppServerEvent => ({
  workspace_id: "workspace-1",
  run_id: "run-1",
  sequence,
  message: { method, params: { itemId: "item-1", status: "completed" } },
});

describe("rememberAppServerEvent", () => {
  it("drops the same Run sequence from live and replay delivery", () => {
    const keys = new Map<string, number>();
    expect(rememberAppServerEvent(keys, event(7))).toBe(true);
    expect(rememberAppServerEvent(keys, event(7))).toBe(false);
  });

  it("keeps identical text when it has distinct durable sequences", () => {
    const keys = new Map<string, number>();
    expect(rememberAppServerEvent(keys, event(7))).toBe(true);
    expect(rememberAppServerEvent(keys, event(8))).toBe(true);
    expect(rememberAppServerEvent(keys, event(9, "item/agentMessage/delta"))).toBe(true);
  });

  it("rejects duplicate and out-of-order sequences for the same Run", () => {
    const keys = new Map<string, number>();
    expect(rememberAppServerEvent(keys, event(10))).toBe(true);
    expect(rememberAppServerEvent(keys, event(10))).toBe(false);
    expect(rememberAppServerEvent(keys, event(9))).toBe(false);
    expect(rememberAppServerEvent(keys, { ...event(1), run_id: "run-2" })).toBe(true);
  });
});
