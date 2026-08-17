import { describe, expect, it } from "vitest";
import type {
  RuntimeAgentActivity,
  RuntimeAgentProjection,
} from "../../browser/types";
import { latestAgentHistoryUpdates } from "./agentWaitUpdates";

function agent(
  threadId: string,
  options: Partial<RuntimeAgentProjection> = {},
): RuntimeAgentProjection {
  return {
    run_id: "run-1",
    thread_id: threadId,
    parent_thread_id: "root-thread",
    source_kind: "subagent",
    agent_path: null,
    agent_nickname: null,
    agent_role: "worker",
    status_type: "active",
    active_flags: [],
    is_root: false,
    first_observed_at: "2026-08-17T00:00:00Z",
    last_observed_at: "2026-08-17T00:00:01Z",
    ...options,
  };
}

function activity(
  threadId: string,
  sequence: number,
  title: string,
  itemId: string | null,
): RuntimeAgentActivity {
  return {
    run_id: "run-1",
    sequence,
    thread_id: threadId,
    turn_id: "turn-1",
    item_id: itemId,
    kind: itemId ? "tool_completed" : "turn_completed",
    status: "completed",
    subject: null,
    title,
    detail: null,
    created_at: "2026-08-17T00:00:01Z",
  };
}

describe("latestAgentHistoryUpdates", () => {
  it("maps every child Agent to its newest Runtime Item and ignores later Turn lifecycle rows", () => {
    const updates = latestAgentHistoryUpdates(
      [
        agent("root-thread", { is_root: true, parent_thread_id: null, agent_nickname: "Root" }),
        agent("data-thread", {
          agent_nickname: "Wanwan",
          agent_role: "data_agent",
          status_type: "completed",
        }),
        agent("network-thread", { agent_nickname: "Euler", agent_role: "network_agent" }),
      ],
      [
        activity("data-thread", 10, "Using inspect_network_sources", "item-data-1"),
        activity("data-thread", 12, "Completed normalize_network_input", "item-data-2"),
        activity("data-thread", 13, "Finished this work cycle", null),
        activity("network-thread", 11, "Using evaluate_network_baseline", "item-network-1"),
        activity("root-thread", 14, "Waiting for Agents", "item-root"),
      ],
    );

    expect(updates).toEqual([
      {
        threadId: "data-thread",
        agentLabel: "Wanwan",
        text: "Completed normalize_network_input",
        status: "completed",
      },
      {
        threadId: "network-thread",
        agentLabel: "Euler",
        text: "Using evaluate_network_baseline",
        status: "active",
      },
    ]);
  });

  it("keeps an Agent visible before its first History Item arrives", () => {
    expect(latestAgentHistoryUpdates([
      agent("new-thread", { agent_role: "network_agent", status_type: "running" }),
    ], [])).toEqual([{
      threadId: "new-thread",
      agentLabel: "network_agent",
      text: "No recorded History item yet.",
      status: "running",
    }]);
  });
});
