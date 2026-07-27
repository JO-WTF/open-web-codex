// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { RuntimeAgentActivity } from "../../../browser/types";
import SupervisorOverview, { buildAgentExecutionNodes } from "./SupervisorOverview";

afterEach(cleanup);

const policy = {
  run_id: "run-1",
  task_id: "task-1",
  thread_id: "root-thread",
  policy_id: "enterprise-supervisor-copilot",
  version: "1.0.0",
  display_name: "Enterprise Supervisor Copilot",
  content_sha256: "a".repeat(64),
  state: "bound" as const,
  created_at: "2026-07-26T00:00:00Z",
  bound_at: "2026-07-26T00:00:01Z",
};

const rootAgent = {
  run_id: "run-1",
  thread_id: "root-thread",
  parent_thread_id: null,
  source_kind: "root",
  agent_path: null,
  agent_nickname: null,
  agent_role: null,
  status_type: "active",
  active_flags: [],
  is_root: true,
  first_observed_at: "2026-07-26T00:00:01Z",
  last_observed_at: "2026-07-26T00:00:02Z",
};

const networkAgent = {
  ...rootAgent,
  thread_id: "network-thread",
  parent_thread_id: "root-thread",
  source_kind: "thread_spawn",
  agent_nickname: "Network Analyst",
  agent_role: "network_planning_agent",
  status_type: "active",
  is_root: false,
  first_observed_at: "2026-07-26T00:00:02Z",
};

function activity(
  sequence: number,
  input: Partial<RuntimeAgentActivity>,
): RuntimeAgentActivity {
  return {
    run_id: "run-1",
    sequence,
    thread_id: "network-thread",
    turn_id: null,
    item_id: `item-${sequence}`,
    kind: "assignment",
    status: "pending",
    title: "Task assigned",
    detail: null,
    created_at: `2026-07-26T00:00:${String(sequence).padStart(2, "0")}Z`,
    ...input,
  };
}

const repeatedExecutions: RuntimeAgentActivity[] = [
  activity(2, {
    detail: "Evaluate the current network plan.",
  }),
  activity(3, {
    turn_id: "turn-network-1",
    kind: "turn_started",
    status: "running",
    title: "Started working",
  }),
  activity(4, {
    turn_id: "turn-network-1",
    kind: "reporting",
    status: "running",
    title: "Reported progress",
    detail: "Validated capacity and demand inputs.",
  }),
  activity(5, {
    turn_id: "turn-network-1",
    kind: "turn_completed",
    status: "completed",
    title: "Finished first work cycle",
  }),
  activity(6, {
    kind: "guidance",
    status: "running",
    title: "Supervisor sent instructions",
    detail: "Compare the feasible network scenarios.",
  }),
  activity(7, {
    turn_id: "turn-network-2",
    kind: "turn_started",
    status: "running",
    title: "Started working",
  }),
  activity(8, {
    turn_id: "turn-network-2",
    kind: "tool_started",
    status: "running",
    title: "Using network planner · compare scenarios",
  }),
];

describe("SupervisorOverview", () => {
  it("renders an empty Agent activity state for a standard Thread", () => {
    render(
      <SupervisorOverview
        taskTitle="Standard task"
        policy={null}
        agents={[]}
        artifacts={[]}
      />,
    );
    expect(screen.getByText("Agent collaboration")).toBeTruthy();
    expect(screen.getByText("No Agent activity yet")).toBeTruthy();
    expect(screen.getByText(
      "Runtime Agents and their task progress will appear here when collaboration starts.",
    )).toBeTruthy();
  });

  it("freezes a completed task and creates a new node when the same Agent runs again", () => {
    const nodes = buildAgentExecutionNodes(
      [rootAgent, networkAgent],
      repeatedExecutions,
    );

    expect(nodes).toHaveLength(2);
    expect(nodes[0]).toMatchObject({
      ordinal: 1,
      task: "Evaluate the current network plan.",
      status: "completed",
      currentBehavior: "Finished first work cycle",
      latestProgress: "Validated capacity and demand inputs.",
    });
    expect(nodes[1]).toMatchObject({
      ordinal: 2,
      task: "Compare the feasible network scenarios.",
      status: "running",
      currentBehavior: "Using network planner · compare scenarios",
    });
  });

  it("does not create a new task node for instructions that never start another Turn", () => {
    const nodes = buildAgentExecutionNodes(
      [rootAgent, networkAgent],
      repeatedExecutions.slice(0, 5).concat(activity(6, {
        kind: "guidance",
        status: "running",
        title: "Supervisor sent instructions",
        detail: "Clarify one assumption without starting another task.",
      })),
    );

    expect(nodes).toHaveLength(1);
    expect(nodes[0]).toMatchObject({
      task: "Evaluate the current network plan.",
      status: "completed",
    });
  });

  it("pins the Supervisor summary above the stream of Agent task executions", () => {
    render(
      <SupervisorOverview
        taskTitle="Optimize the enterprise supply-chain network"
        policy={policy}
        agents={[rootAgent, networkAgent]}
        activities={[
          activity(1, {
            thread_id: "root-thread",
            turn_id: "turn-root",
            kind: "reporting",
            status: "running",
            title: "Reported progress",
            detail: "Decomposed the objective and assigned the first specialist.",
          }),
          ...repeatedExecutions,
        ]}
        artifacts={[
          {
            id: "artifact-1",
            task_id: "task-1",
            artifact_schema: "planning-dataset.v1",
            display_name: "planning-dataset.v1",
            mime_type: "application/json",
            expected_size: 2048,
            byte_size: 2048,
            content_sha256: "b".repeat(64),
            state: "ready",
            producer_run_id: "run-1",
            producer_thread_id: "data-thread",
            producer_turn_id: "turn-data",
            producer_item_id: "item-data",
            producer_agent_role: "data_agent",
            created_at: "2026-07-26T00:00:03Z",
            updated_at: "2026-07-26T00:00:04Z",
          },
        ]}
      />,
    );

    expect(screen.getByText("Enterprise Supervisor Copilot")).toBeTruthy();
    expect(screen.getByText("Policy enterprise-supervisor-copilot · 1.0.0")).toBeTruthy();
    expect(screen.getByRole("article", { name: "Supervisor status" })).toBeTruthy();
    expect(screen.getByText("Optimize the enterprise supply-chain network")).toBeTruthy();
    expect(screen.getByText(
      "Decomposed the objective and assigned the first specialist.",
    )).toBeTruthy();
    expect(screen.getByText("Agent task stream")).toBeTruthy();
    expect(screen.getAllByText("Network Analyst")).toHaveLength(2);
    expect(screen.getByText("network_planning_agent · Task 1")).toBeTruthy();
    expect(screen.getByText("network_planning_agent · Task 2")).toBeTruthy();
    expect(screen.getByText("Validated capacity and demand inputs.")).toBeTruthy();
    expect(screen.getByText("Using network planner · compare scenarios")).toBeTruthy();
    expect(screen.getByText("Evidence Artifacts")).toBeTruthy();
    expect(screen.getByText("planning-dataset.v1")).toBeTruthy();
    expect(screen.getByText("data_agent · 2,048 bytes")).toBeTruthy();
  });
});
