// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  RuntimeAgentActivity,
  RuntimeAgentExecution,
} from "../../../browser/types";
import SupervisorOverview from "./SupervisorOverview";

afterEach(cleanup);

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
  agent_nickname: "Network Agent",
  agent_role: "network_planning_agent",
  status_type: "active",
  is_root: false,
  first_observed_at: "2026-07-26T00:00:02Z",
};

const dataAgent = {
  ...networkAgent,
  thread_id: "data-thread",
  agent_nickname: "Data Agent",
  agent_role: "data_agent",
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

function execution(
  id: string,
  input: Partial<RuntimeAgentExecution>,
): RuntimeAgentExecution {
  return {
    id,
    run_id: "run-1",
    thread_id: "network-thread",
    turn_id: `turn-${id}`,
    ordinal: 1,
    task: "Evaluate the current network plan.",
    status: "running",
    current_behavior: "Started working",
    latest_progress: null,
    display_title: "Network Agent · Evaluate the current network plan",
    result_summary: null,
    waiting_approval_id: null,
    wait_cycle_count: 0,
    first_observed_sequence: 2,
    last_observed_sequence: 3,
    started_at: "2026-07-26T00:00:02Z",
    completed_at: null,
    created_at: "2026-07-26T00:00:02Z",
    updated_at: "2026-07-26T00:00:03Z",
    ...input,
  };
}

const repeatedExecutions: RuntimeAgentExecution[] = [
  execution("network-1", {
    turn_id: "turn-network-1",
    ordinal: 1,
    status: "completed",
    current_behavior: "Finished this work cycle",
    latest_progress: "Validated capacity and demand inputs.",
    last_observed_sequence: 5,
    completed_at: "2026-07-26T00:00:05Z",
  }),
  execution("network-2", {
    turn_id: "turn-network-2",
    ordinal: 2,
    task: "Compare the feasible network scenarios.",
    current_behavior: "Using network planner · compare scenarios",
    first_observed_sequence: 6,
    last_observed_sequence: 8,
  }),
];

describe("SupervisorOverview", () => {
  it("renders an empty Agent activity state for a standard Thread", () => {
    render(
      <SupervisorOverview
        taskTitle="Standard task"
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

  it("renders a frozen completed task and a new node when the same Agent runs again", () => {
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        executions={repeatedExecutions}
        artifacts={[]}
      />,
    );

    expect(screen.getByText("network_planning_agent · Task 1")).toBeTruthy();
    expect(screen.getByText("network_planning_agent · Task 2")).toBeTruthy();
    expect(screen.getByText("Validated capacity and demand inputs.")).toBeTruthy();
    expect(screen.getByText("Using network planner · compare scenarios")).toBeTruthy();
  });

  it("renders a child Agent approval as an actionable approval card", () => {
    const onResolveApproval = vi.fn();
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, dataAgent]}
        approvals={[{
          threadId: "data-thread",
          actorLabel: "Data Agent",
          workspaceId: "workspace-1",
          requestId: "approval-1",
          command: "Allow supply chain data · list planning sources?",
          status: "pending",
          serverName: "supply_chain_data",
        }]}
        onResolveApproval={onResolveApproval}
        artifacts={[]}
      />,
    );

    const queue = screen.getByRole("region", { name: "Agent approvals" });
    expect(queue.textContent).toContain("Data Agent");
    expect(queue.textContent).toContain("Approval required");
    expect(queue.textContent).toContain("Allow supply chain data");

    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    expect(onResolveApproval).toHaveBeenCalledWith(
      "workspace-1",
      "approval-1",
      "accept",
    );
  });

  it("renders parallel Agent executions as independent task nodes", () => {
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, dataAgent, networkAgent]}
        executions={[
          execution("data-1", {
          thread_id: "data-thread",
          turn_id: "turn-data-1",
          task: "Validate the planning dataset.",
          display_title: "Data Agent · Validate the planning dataset",
          first_observed_sequence: 2,
        }),
          execution("network-1", {
            first_observed_sequence: 3,
          }),
        ]}
        artifacts={[]}
      />,
    );

    expect(screen.getByText("Data Agent · Validate the planning dataset")).toBeTruthy();
    expect(screen.getByText("Network Agent · Evaluate the current network plan")).toBeTruthy();
    expect(screen.getByText("Validate the planning dataset.")).toBeTruthy();
    expect(screen.getByText("Evaluate the current network plan.")).toBeTruthy();
  });

  it("renders persisted Supervisor and child Agent behavior in sequence", () => {
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, dataAgent]}
        activities={[
          activity(11, {
            thread_id: "root-thread",
            kind: "waiting",
            status: "waiting",
            title: "Waiting for Agent updates",
            detail: "This is a bounded wait, not a stopped task.",
          }),
          activity(10, {
            thread_id: "data-thread",
            kind: "tool_completed",
            status: "completed",
            title: "Completed supply chain data · validate inputs",
            detail: null,
          }),
          activity(12, {
            thread_id: "root-thread",
            kind: "waiting",
            status: "completed",
            title: "Wait cycle finished",
            detail: "The Supervisor may start another wait cycle.",
          }),
        ]}
        artifacts={[]}
      />,
    );

    const log = screen.getByLabelText("Agent behavior log").querySelector("ol");
    expect(screen.getByText("Agent behavior log")).toBeTruthy();
    expect(screen.getAllByText("Root Supervisor")).toHaveLength(3);
    expect(screen.getByText("Data Agent")).toBeTruthy();
    expect(screen.getByText("Waiting for Agent updates")).toBeTruthy();
    expect(screen.getByText("This is a bounded wait, not a stopped task.")).toBeTruthy();
    expect(screen.getByText("Wait cycle finished")).toBeTruthy();
    const logText = log?.textContent ?? "";
    expect(logText.indexOf("Completed supply chain data")).toBeLessThan(
      logText.indexOf("Waiting for Agent updates"),
    );
  });

  it("pins the Supervisor summary above the stream of Agent task executions", () => {
    render(
      <SupervisorOverview
        taskTitle="Optimize the enterprise supply-chain network"
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
        ]}
        executions={repeatedExecutions}
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

    expect(screen.getByText("Agent collaboration")).toBeTruthy();
    expect(screen.getByText("Runtime-owned Agent collaboration")).toBeTruthy();
    expect(screen.getByRole("article", { name: "Supervisor status" })).toBeTruthy();
    expect(screen.getByText("Optimize the enterprise supply-chain network")).toBeTruthy();
    expect(screen.getAllByText(
      "Decomposed the objective and assigned the first specialist.",
    )).toHaveLength(2);
    expect(screen.getByText("Agent task stream")).toBeTruthy();
    expect(screen.getAllByText(/^Network Agent ·/)).toHaveLength(2);
    expect(screen.getByText("network_planning_agent · Task 1")).toBeTruthy();
    expect(screen.getByText("network_planning_agent · Task 2")).toBeTruthy();
    expect(screen.getByText("Validated capacity and demand inputs.")).toBeTruthy();
    expect(screen.getByText("Using network planner · compare scenarios")).toBeTruthy();
    expect(screen.getByText("Evidence Artifacts")).toBeTruthy();
    expect(screen.getByText("planning-dataset.v1")).toBeTruthy();
    expect(screen.getByText("data_agent · 2,048 bytes")).toBeTruthy();
  });

  it("opens authoritative child Agent history without changing the root Thread", async () => {
    const onLoadAgentHistory = vi.fn().mockResolvedValue([{
      id: "turn-network-2",
      status: "completed",
      items: [{
        id: "message-network-2",
        type: "agentMessage",
        text: "The follow-up scenario remains feasible.",
      }],
    }]);
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        executions={repeatedExecutions}
        artifacts={[]}
        onLoadAgentHistory={onLoadAgentHistory}
      />,
    );

    fireEvent.click(screen.getAllByRole("button", {
      name: "Review Agent history",
    })[1]);
    expect(onLoadAgentHistory).toHaveBeenCalledWith("network-thread");
    expect(await screen.findByText("The follow-up scenario remains feasible.")).toBeTruthy();
    expect(screen.getByRole("dialog").textContent).toContain("Turn 1");
  });

  it("opens ready Artifact content through the authorized loader", async () => {
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8666";
    const onLoadArtifactContent = vi.fn().mockResolvedValue({
      schema_version: "planning-dataset.v1",
      demand_nodes: 12,
    });
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, dataAgent]}
        artifacts={[{
          id: artifactId,
          task_id: "task-1",
          artifact_schema: "planning-dataset.v1",
          display_name: "Validated planning dataset",
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
        }]}
        onLoadArtifactContent={onLoadArtifactContent}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(onLoadArtifactContent).toHaveBeenCalledWith(artifactId);
    expect(await screen.findByText(/"demand_nodes": 12/)).toBeTruthy();
    expect(screen.getByRole("dialog").textContent).toContain("Authorized Artifact");
  });
});
