// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RunReadiness } from "../../../browser/types";
import Workspaces from "./Workspaces";

const ready: RunReadiness = {
  status: "ready",
  evaluation_fingerprint: "ready-fingerprint",
  checks: [],
};

const workspace = {
  id: "ws-1",
  name: "Demo",
  path: "/tmp/demo",
  connected: true,
  settings: { sidebarCollapsed: false },
};

function baseProps() {
  return {
    workspaces: [workspace],
    activeId: "ws-1",
    onSelect: vi.fn(),
    onCreate: vi.fn(),
    onConnect: vi.fn(),
    onLoad: vi.fn(),
    busy: false,
    threadsByWorkspace: { "ws-1": [] },
    activeThreadId: null,
    onSelectThread: vi.fn(),
    onEvaluateReadiness: vi.fn(async () => ready),
    onStartTask: vi.fn(async () => true),
    onReadinessAction: vi.fn(),
    onArchiveThread: vi.fn(),
    onRemoveWorkspace: vi.fn(),
  };
}

describe("Web workspace actions", () => {
  afterEach(cleanup);
  it("places the remove action before the single New task action", () => {
    const props = baseProps();
    render(<Workspaces {...props} />);

    const remove = screen.getByRole("button", { name: "Remove workspace Demo" });
    const addThread = screen.getByRole("button", { name: "New task in Demo" });
    expect(
      remove.compareDocumentPosition(addThread) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(remove);
    expect(props.onRemoveWorkspace).toHaveBeenCalledWith("ws-1");
  });

  it("checks readiness and starts the exact Supervisor selected in the unified launcher", async () => {
    const props = baseProps();
    const policy = {
      policy_id: "enterprise-supervisor-copilot",
      version: "1.1.0",
      display_name: "Enterprise Supervisor Copilot",
      description: "Coordinates governed data and network planning.",
      source: "repository" as const,
    };
    render(<Workspaces {...props} supervisorPolicies={[policy]} />);

    fireEvent.click(screen.getByRole("button", { name: "New task in Demo" }));
    expect(screen.getByRole("dialog", { name: "Create Thread" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Supervisor/ }));
    fireEvent.click(
      screen.getByRole("button", { name: /Enterprise Supervisor Copilot/ }),
    );

    await waitFor(() =>
      expect(props.onEvaluateReadiness).toHaveBeenCalledWith("ws-1", {
        kind: "supervisor",
        policy,
      }),
    );
    await waitFor(() =>
      expect(
        (screen.getByRole("button", { name: "Create Thread" }) as HTMLButtonElement)
          .disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole("button", { name: "Create Thread" }));
    await waitFor(() =>
      expect(props.onStartTask).toHaveBeenCalledWith(
        "ws-1",
        { kind: "supervisor", policy },
        ready,
        expect.any(String),
      ),
    );
  });

  it("restores focus to the New task button that opened the launcher", async () => {
    const props = baseProps();
    render(<Workspaces {...props} />);

    const trigger = screen.getByRole("button", { name: "New task in Demo" });
    fireEvent.click(trigger);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });

  it("offers only an Agent compatible with the selected Workspace", async () => {
    const props = baseProps();
    const compatibleAgent = {
      source: "user_release" as const,
      release_id: "agent-release-1",
      definition_id: "network-agent",
      version: "2.0.0",
      display_name: "Network Agent",
      description: "Reviews the current network.",
      responsibilities: ["Analyze service coverage"],
      input_artifact_types: [],
      output_artifact_types: ["NetworkAssessment"],
      required_capabilities: ["network_planning.analyze"],
      capability_template: null,
      dataset_releases: [],
      required_workspace_id: "ws-1",
    };
    const incompatibleAgent = {
      ...compatibleAgent,
      release_id: "agent-release-2",
      definition_id: "other-agent",
      display_name: "Other Workspace Agent",
      required_workspace_id: "ws-2",
    };
    render(
      <Workspaces
        {...props}
        agents={[compatibleAgent, incompatibleAgent]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "New task in Demo" }));
    fireEvent.click(screen.getByText("Agent", { selector: "strong" }).closest("button")!);
    expect(screen.queryByText("Other Workspace Agent")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /Network Agent/ }));

    await waitFor(() =>
      expect(props.onEvaluateReadiness).toHaveBeenCalledWith("ws-1", {
        kind: "agent",
        agent: compatibleAgent,
      }),
    );
  });

  it("shows running state and confirms before archiving a thread", () => {
    const props = baseProps();
    const onArchiveThread = vi.fn();
    render(
      <Workspaces
        {...props}
        threadsByWorkspace={{
          "ws-1": [
            {
              id: "running-thread",
              label: "Running",
              updatedAt: 2,
              status: "running",
            },
            {
              id: "idle-thread",
              label: "Idle",
              updatedAt: 1,
              status: "idle",
            },
          ],
        }}
        onArchiveThread={onArchiveThread}
      />,
    );

    const runningIndicator = screen.getByRole("status", {
      name: "Thread is running",
    });
    const runningLabel = screen.getByText("Running");
    expect(
      runningLabel.compareDocumentPosition(runningIndicator) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(document.querySelector(".web-ws-thread-status.is-running")).toBeNull();
    expect(
      screen.getByText("Idle").parentElement?.querySelector(
        ".web-ws-thread-running",
      ),
    ).toBeNull();
    expect(
      (screen.getByRole("button", {
        name: "Archive thread Running",
      }) as HTMLButtonElement).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Archive thread Idle" }));
    expect(
      screen.getByRole("alertdialog", { name: "Archive thread?" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Archive" }));
    expect(onArchiveThread).toHaveBeenCalledWith("ws-1", "idle-thread");
  });
});
