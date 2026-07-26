// @vitest-environment jsdom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import SupervisorOverview from "./SupervisorOverview";

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

describe("SupervisorOverview", () => {
  it("renders nothing for a standard Thread", () => {
    const view = render(<SupervisorOverview policy={null} agents={[]} artifacts={[]} />);
    expect(view.container.childElementCount).toBe(0);
  });

  it("shows the bound Policy and Runtime-owned Agent statuses", () => {
    render(
      <SupervisorOverview
        policy={policy}
        agents={[
          rootAgent,
          {
            ...rootAgent,
            thread_id: "network-thread",
            parent_thread_id: "root-thread",
            source_kind: "thread_spawn",
            agent_nickname: "Network Analyst",
            agent_role: "network_planning_agent",
            status_type: "idle",
            is_root: false,
            first_observed_at: "2026-07-26T00:00:02Z",
          },
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
    expect(screen.getByText("Root Supervisor")).toBeTruthy();
    expect(screen.getByText("Network Analyst")).toBeTruthy();
    expect(screen.getByText("network_planning_agent")).toBeTruthy();
    expect(screen.getByText("Running")).toBeTruthy();
    expect(screen.getByText("Ready")).toBeTruthy();
    expect(screen.getByText("Evidence Artifacts")).toBeTruthy();
    expect(screen.getByText("planning-dataset.v1")).toBeTruthy();
    expect(screen.getByText("data_agent · 2,048 bytes")).toBeTruthy();
  });
});
