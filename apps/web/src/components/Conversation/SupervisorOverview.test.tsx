// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  RuntimeAgentActivity,
  RuntimeAgentExecution,
} from "../../../browser/types";
import SupervisorOverview, { orderAndDedupeActivities } from "./SupervisorOverview";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

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
    subject: null,
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
    const summary = screen.getByLabelText("Agent collaboration summary");
    expect(summary.textContent).toContain("2 Agents");
    expect(summary.textContent).toContain("2 active");
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

    const behaviorLog = screen.getByLabelText("Agent behavior log") as HTMLDetailsElement;
    const log = behaviorLog.querySelector("ol");
    expect(behaviorLog.open).toBe(false);
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
            failure: null,
            content_url: "/api/artifacts/artifact-1/content",
            download_url: "/api/artifacts/artifact-1/download",
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
    expect(screen.getByText("Live Runtime collaboration")).toBeTruthy();
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
    expect(screen.getByText("Final deliveries")).toBeTruthy();
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
      kind: "json",
      mime_type: "application/json",
      value: {
        schema_version: "planning-dataset.v1",
        demand_nodes: 12,
      },
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
          failure: null,
          content_url: `/api/artifacts/${artifactId}/content`,
          download_url: `/api/artifacts/${artifactId}/download`,
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

  it("opens a Markdown report as readable authorized content", async () => {
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8667";
    const onLoadArtifactContent = vi.fn().mockResolvedValue({
      kind: "markdown",
      mime_type: "text/markdown",
      text: [
        "# Warehouse network planning report",
        "",
        "## Executive summary",
        "",
        "- Planned city coverage: **92.0%**",
      ].join("\n"),
    });
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        artifacts={[{
          id: artifactId,
          task_id: "task-1",
          artifact_schema: "network_planning_report_markdown.v1",
          display_name: "Warehouse network planning report",
          mime_type: "text/markdown",
          expected_size: 1024,
          byte_size: 1024,
          content_sha256: "c".repeat(64),
          state: "ready",
          failure: null,
          content_url: `/api/artifacts/${artifactId}/content`,
          download_url: `/api/artifacts/${artifactId}/download`,
          producer_run_id: "run-1",
          producer_thread_id: "network-thread",
          producer_turn_id: "turn-network",
          producer_item_id: "item-report",
          producer_agent_role: "network_agent",
          created_at: "2026-08-11T00:00:03Z",
          updated_at: "2026-08-11T00:00:04Z",
        }]}
        onLoadArtifactContent={onLoadArtifactContent}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(onLoadArtifactContent).toHaveBeenCalledWith(artifactId);
    expect((await screen.findAllByRole("heading", {
      name: "Warehouse network planning report",
    })).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/Planned city coverage:/).textContent).toContain("92.0%");
    expect(screen.queryByText(/"kind": "markdown"/)).toBeNull();
  });

  it("downloads a ready final delivery through the authorized client", async () => {
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8668";
    const onDownloadArtifact = vi.fn().mockResolvedValue({
      blob: new Blob(["# Brief\n"], { type: "text/markdown" }),
      filename: "warehouse-network-brief.md",
    });
    const createObjectURL = vi.fn().mockReturnValue("blob:network-report");
    const revokeObjectURL = vi.fn();
    vi.stubGlobal("URL", { createObjectURL, revokeObjectURL });
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => {});
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        artifacts={[{
          id: artifactId,
          task_id: "task-1",
          artifact_schema: "network_planning_report_markdown.v1",
          display_name: "Warehouse network planning report",
          mime_type: "text/markdown",
          expected_size: 128,
          byte_size: 128,
          content_sha256: "d".repeat(64),
          state: "ready",
          failure: null,
          content_url: `/api/artifacts/${artifactId}/content`,
          download_url: `/api/artifacts/${artifactId}/download`,
          producer_run_id: "run-1",
          producer_thread_id: "network-thread",
          producer_turn_id: "turn-network",
          producer_item_id: "item-report",
          producer_agent_role: "network_agent",
          created_at: "2026-08-11T00:00:03Z",
          updated_at: "2026-08-11T00:00:04Z",
        }]}
        onDownloadArtifact={onDownloadArtifact}
      />,
    );

    fireEvent.click(screen.getByRole("button", {
      name: "Download Warehouse network planning report",
    }));
    expect(onDownloadArtifact).toHaveBeenCalledWith(artifactId);
    await waitFor(() => expect(createObjectURL).toHaveBeenCalledTimes(1));
    expect(click).toHaveBeenCalledTimes(1);
  });

  it("renders native Runtime subjects as concrete root and child actions", () => {
    const rawActivity = {
      ...activity(20, {
        thread_id: "root-thread",
        turn_id: "turn-root",
        item_id: "item-workspace",
        kind: "tool_completed",
        status: "completed",
        subject: {
          kind: "workspace_action",
          action: "read",
          path: "inputs/planning.csv",
        },
        title: "Completed a workspace command",
        detail: "read · inputs/planning.csv",
      }),
      raw_command: "rm -rf /private/secret",
      raw_output: "DEEPSEEK_KEY=do-not-render",
    };
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        activities={[
          activity(10, {
            thread_id: "network-thread",
            turn_id: "turn-network",
            item_id: "item-mcp",
            kind: "tool_started",
            status: "running",
            subject: {
              kind: "mcp_tool",
              server: "supply_chain",
              tool: "list_sources",
            },
            title: "Using a workspace command",
          }),
          activity(11, {
            thread_id: "network-thread",
            turn_id: "turn-network",
            item_id: "item-runtime",
            kind: "tool_completed",
            status: "completed",
            subject: {
              kind: "runtime_tool",
              namespace: "network",
              tool: "compare_routes",
            },
            title: "Completed a workspace command",
          }),
          rawActivity,
          activity(21, {
            thread_id: "network-thread",
            turn_id: "turn-network",
            item_id: "item-search",
            kind: "tool_completed",
            status: "completed",
            subject: { kind: "web_search" },
            title: "Completed web search",
          }),
          activity(22, {
            thread_id: "network-thread",
            turn_id: "turn-network",
            item_id: "item-image",
            kind: "tool_completed",
            status: "completed",
            subject: { kind: "image_view" },
            title: "Completed image inspection",
          }),
          activity(23, {
            thread_id: "network-thread",
            turn_id: "turn-network",
            item_id: "item-generation",
            kind: "tool_failed",
            status: "failed",
            subject: { kind: "image_generation" },
            title: "Could not complete image generation",
            detail: "The image provider declined the request.",
          }),
          activity(24, {
            thread_id: "root-thread",
            turn_id: "turn-root",
            item_id: "item-wait",
            kind: "waiting",
            status: "waiting",
            title: "Waiting for Agent updates",
          }),
          activity(25, {
            thread_id: "root-thread",
            turn_id: "turn-root",
            item_id: "item-workspace-fallback",
            kind: "tool_started",
            status: "running",
            subject: {
              kind: "workspace_action",
              action: "workspace_command",
              path: null,
            },
            title: "Using a workspace command",
          }),
        ]}
        artifacts={[]}
      />,
    );

    expect(screen.getByText("supply_chain · list_sources")).toBeTruthy();
    expect(screen.getByText("network · compare_routes")).toBeTruthy();
    expect(screen.getAllByText("read · inputs/planning.csv").length).toBeGreaterThan(0);
    expect(screen.getAllByText("workspace operation").length).toBeGreaterThan(0);
    expect(screen.getByText("web search")).toBeTruthy();
    expect(screen.getByText("image inspection")).toBeTruthy();
    expect(screen.getByText("image generation")).toBeTruthy();
    expect(screen.getAllByText("Root Supervisor").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Network Agent").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Waiting").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Completed").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Failed").length).toBeGreaterThan(0);
    expect(screen.queryByText("Using a workspace command")).toBeNull();
    expect(screen.queryByText("Completed a workspace command")).toBeNull();
    expect(screen.queryByText("rm -rf /private/secret")).toBeNull();
    expect(screen.queryByText("DEEPSEEK_KEY=do-not-render")).toBeNull();
    const log = screen.getByLabelText("Agent behavior log");
    expect((log as HTMLDetailsElement).open).toBe(true);
    expect(log.querySelector("[role=alert], [role=status], [aria-live]")).toBeNull();
    expect(log.textContent).toContain("image generation");
    expect(log.textContent).toContain("Waiting");
    expect(log.textContent).toContain("Failed");
  });

  it("sorts by Runtime sequence and removes replay duplicates by official identity", () => {
    const earlier = activity(2, {
      thread_id: "root-thread",
      turn_id: "turn-root",
      item_id: "item-root",
      kind: "turn_started",
      status: "running",
      title: "Root turn started",
    });
    const later = activity(7, {
      thread_id: "network-thread",
      turn_id: "turn-network",
      item_id: "item-network",
      kind: "tool_completed",
      status: "completed",
      subject: {
        kind: "mcp_tool",
        server: "supply_chain",
        tool: "calculate_routes",
      },
      title: "Completed a tool",
    });
    const ordered = orderAndDedupeActivities([later, { ...earlier }, earlier, later]);
    expect(ordered).toHaveLength(2);
    expect(ordered.map((entry) => entry.sequence)).toEqual([2, 7]);

    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        activities={[later, earlier, { ...later }, { ...earlier }]}
        artifacts={[]}
      />,
    );

    const log = screen.getByLabelText("Agent behavior log");
    const items = log.querySelectorAll("ol > li");
    expect(items).toHaveLength(2);
    expect(items[0]?.textContent).toContain("Root turn started");
    expect(items[1]?.textContent).toContain("supply_chain · calculate_routes");
  });

  it("uses unique useId DOM references for duplicate sequence and Item values", () => {
    const shared = {
      sequence: 50,
      turn_id: "turn-shared",
      item_id: "item-shared",
      kind: "tool_completed" as const,
      status: "completed" as const,
    };
    const { container } = render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, dataAgent, networkAgent]}
        activities={[
          activity(50, {
            ...shared,
            thread_id: "data-thread",
            subject: {
              kind: "mcp_tool",
              server: "supply_chain",
              tool: "validate_data",
            },
            title: "Completed a tool",
          }),
          activity(50, {
            ...shared,
            thread_id: "network-thread",
            subject: {
              kind: "runtime_tool",
              namespace: "network",
              tool: "compare_routes",
            },
            title: "Completed a tool",
          }),
        ]}
        artifacts={[]}
      />,
    );

    const ids = [...container.querySelectorAll<HTMLElement>("[id]")]
      .map((element) => element.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids.some((id) => id.includes("item-shared") || id.includes("data-thread"))).toBe(false);

    const log = screen.getByLabelText("Agent behavior log");
    const rows = [...log.querySelectorAll<HTMLElement>(".web-supervisor-activity-card")];
    expect(rows).toHaveLength(2);
    expect(screen.getByRole("article", { name: /Data Agent.*supply_chain · validate_data/ })).toBeTruthy();
    expect(screen.getByRole("article", { name: /Network Agent.*network · compare_routes/ })).toBeTruthy();
    for (const row of rows) {
      const references = row.getAttribute("aria-labelledby")?.split(" ") ?? [];
      expect(references.length).toBe(2);
      expect(references.every((id) => row.ownerDocument.getElementById(id))).toBe(true);
      expect(row.ownerDocument.getElementById(row.getAttribute("aria-describedby") ?? "")).toBeTruthy();
    }
  });

  it("merges started and completed events for one Runtime Item", () => {
    const subject = {
      kind: "mcp_tool" as const,
      server: "supply_chain",
      tool: "validate_inputs",
    };
    const started = activity(40, {
      thread_id: "network-thread",
      turn_id: "turn-network",
      item_id: "item-validate",
      kind: "tool_started",
      status: "running",
      subject,
      title: "Using a tool",
    });
    const completed = activity(41, {
      thread_id: "network-thread",
      turn_id: "turn-network",
      item_id: "item-validate",
      kind: "tool_completed",
      status: "completed",
      subject,
      title: "Completed a tool",
    });
    const otherCall = activity(42, {
      thread_id: "network-thread",
      turn_id: "turn-network",
      item_id: "item-validate-again",
      kind: "tool_completed",
      status: "completed",
      subject,
      title: "Completed a tool again",
    });
    const merged = orderAndDedupeActivities([completed, started, { ...started }, otherCall]);
    expect(merged).toHaveLength(2);
    expect(merged[0]).toEqual(completed);
    expect(merged[1]).toEqual(otherCall);

    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        activities={[completed, started, { ...started }, otherCall]}
        artifacts={[]}
      />,
    );

    const log = screen.getByLabelText("Agent behavior log");
    expect(log.querySelectorAll("ol > li")).toHaveLength(2);
    expect(log.textContent).not.toContain("Running");
    expect(screen.getAllByText("Completed")).toHaveLength(2);
  });

  it("expands and focuses the behavior log for an Agent activity request", () => {
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        activities={[activity(41, {
          thread_id: "network-thread",
          turn_id: "turn-network",
          item_id: "item-network",
          kind: "tool_completed",
          status: "completed",
          title: "Completed evaluate_network_baseline",
        })]}
        artifacts={[]}
        behaviorLogFocusRequest={1}
      />,
    );

    const behaviorLog = screen.getByLabelText("Agent behavior log") as HTMLDetailsElement;
    expect(behaviorLog.open).toBe(true);
    expect(document.activeElement).toBe(behaviorLog.querySelector("summary"));
  });

  it("shows Thinking during live reasoning and refreshes that same row on completion", () => {
    const started = activity(40, {
      thread_id: "network-thread",
      turn_id: "turn-network",
      item_id: "reasoning-network",
      kind: "reasoning",
      status: "running",
      title: "Reasoning: partial streamed text must not be shown.",
      detail: "partial streamed text must not be shown.",
    });
    const completed = activity(41, {
      thread_id: "network-thread",
      turn_id: "turn-network",
      item_id: "reasoning-network",
      kind: "reasoning",
      status: "completed",
      title: "Reasoning: Checking coverage and capacity.",
      detail: "Checking coverage and capacity.",
    });

    expect(orderAndDedupeActivities([completed, started])).toEqual([completed]);

    const { rerender } = render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        activities={[started]}
        artifacts={[]}
      />,
    );

    const behaviorLog = screen.getByLabelText("Agent behavior log");
    const liveRow = screen.getByRole("article", { name: /Network Agent.*Thinking/ });
    expect(behaviorLog.textContent).toContain("Thinking");
    expect(behaviorLog.textContent).not.toContain("partial streamed text");

    rerender(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        activities={[started, completed]}
        artifacts={[]}
      />,
    );

    const completedRow = screen.getByRole("article", {
      name: /Network Agent.*Reasoning: Checking coverage and capacity/,
    });
    expect(completedRow).toBe(liveRow);
    expect(behaviorLog.textContent).toContain("Completed");
    expect(behaviorLog.textContent).toContain("Reasoning: Checking coverage and capacity.");
  });

  it("shows bounded Runtime-provided reasoning text", () => {
    render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, networkAgent]}
        activities={[activity(41, {
          thread_id: "network-thread",
          turn_id: "turn-network",
          item_id: "reasoning-network",
          kind: "reasoning",
          status: "completed",
          title: "Reasoning: Checking coverage and capacity.",
          detail: "Checking coverage and capacity.",
        })]}
        artifacts={[]}
      />,
    );

    const log = screen.getByLabelText("Agent behavior log");
    expect(log.textContent).toContain("Reasoning: Checking coverage and capacity.");
    fireEvent.click(screen.getByRole("button", { name: "Show safe activity detail" }));
    expect(screen.getByText("Checking coverage and capacity.")).toBeTruthy();
  });

  it("keeps safe detail keyboard-expandable and exposes status semantics", () => {
    const detail = "search · inputs/planning.csv";
    const { container } = render(
      <SupervisorOverview
        taskTitle="Network planning"
        agents={[rootAgent, dataAgent]}
        activities={[activity(30, {
          thread_id: "data-thread",
          turn_id: "turn-data",
          item_id: "item-read",
          kind: "tool_completed",
          status: "completed",
          subject: {
            kind: "workspace_action",
            action: "search",
            path: "inputs/planning.csv",
          },
          title: "Completed a workspace command",
          detail,
        }), activity(31, {
          thread_id: "data-thread",
          turn_id: "turn-data",
          item_id: "item-failed",
          kind: "tool_failed",
          status: "failed",
          subject: {
            kind: "runtime_tool",
            namespace: "data",
            tool: "validate",
          },
          title: "Could not complete validation",
        })]}
        artifacts={[]}
      />,
    );

    const summary = container.querySelector(
      ".web-supervisor-activity-detail summary",
    ) as HTMLElement | null;
    expect(summary).toBeTruthy();
    expect(screen.getByRole("button", { name: "Show command details" })).toBeTruthy();
    expect(summary?.getAttribute("aria-label")).toBe("Show command details");
    expect((summary?.closest("details") as HTMLDetailsElement | null)?.open).toBe(false);
    fireEvent.click(summary as HTMLElement);
    expect((summary?.closest("details") as HTMLDetailsElement | null)?.open).toBe(true);
    expect(screen.getAllByText(detail).length).toBeGreaterThan(0);
    expect(container.querySelector(".web-supervisor-activity-action")).toBeTruthy();
    expect(screen.getByText("data · validate")).toBeTruthy();
    expect(screen.getByLabelText("Status: Completed")).toBeTruthy();
    expect(screen.getByLabelText("Status: Failed")).toBeTruthy();
  });
});
