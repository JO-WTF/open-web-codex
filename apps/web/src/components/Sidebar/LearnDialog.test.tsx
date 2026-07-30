// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RunReadiness } from "../../../browser/types";
import LearnDialog from "./LearnDialog";

const {
  listTutorialBlueprints,
  getTutorialBlueprint,
  reconcileTutorialBlueprint,
} = vi.hoisted(() => ({
  listTutorialBlueprints: vi.fn(),
  getTutorialBlueprint: vi.fn(),
  reconcileTutorialBlueprint: vi.fn(),
}));

vi.mock("../../../browser/session", () => ({
  platformClient: {
    listTutorialBlueprints,
    getTutorialBlueprint,
    reconcileTutorialBlueprint,
  },
}));

const blueprint = {
  blueprint_id: "network-case",
  revision: "1.0.0",
  display_name: "Prepared network case",
  description: "A governed tutorial.",
  estimated_minutes: 10,
  dataset: {
    dataset_id: "network",
    version: "1.0.0",
    display_name: "Network inputs",
    description: "Inputs",
    file_count: 3,
    source_content_sha256: "a".repeat(64),
  },
  agent_templates: [{
    definition_id: "network-agent",
    version: "1.0.0",
    content_sha256: "b".repeat(64),
  }],
  supervisor_template: {
    policy_id: "network-supervisor",
    version: "1.0.0",
    content_sha256: "c".repeat(64),
  },
  instruction_policy_template: {
    policy_id: "platform-supervisor",
    version: "1.0.0",
    content_sha256: "d".repeat(64),
  },
  required_mcp_servers: ["map_utils"],
  expected_artifact_types: ["report.v1", "map.v3"],
  recommended_prompt: "Analyze the prepared network.",
  content_sha256: "e".repeat(64),
};

const ready: RunReadiness = {
  status: "ready",
  evaluation_fingerprint: "ready",
  checks: [],
};

const workspace = {
  id: "workspace-1",
  name: "Demo",
  path: "Demo",
  connected: true,
  settings: { sidebarCollapsed: false },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("LearnDialog", () => {
  afterEach(cleanup);
  beforeEach(() => {
    vi.clearAllMocks();
    listTutorialBlueprints.mockResolvedValue([{
      blueprint_id: blueprint.blueprint_id,
      revision: blueprint.revision,
      display_name: blueprint.display_name,
      description: blueprint.description,
      estimated_minutes: blueprint.estimated_minutes,
    }]);
    getTutorialBlueprint.mockResolvedValue(blueprint);
    reconcileTutorialBlueprint.mockResolvedValue({
      status: "installed",
      blueprint_id: blueprint.blueprint_id,
      revision: blueprint.revision,
      workspace_id: "workspace-1",
      dataset_release: null,
      agent_releases: [],
      supervisor_release: null,
      supervisor_policy: {
        policy_id: "network-supervisor",
        version: "1.0.0",
      },
      recommended_prompt: blueprint.recommended_prompt,
      expected_artifact_types: blueprint.expected_artifact_types,
      issues: [],
    });
  });

  it("installs exact resources, checks readiness, and loads the recommended prompt after start", async () => {
    const onEvaluateReadiness = vi.fn(async () => ready);
    const onStartTask = vi.fn(async () => true);
    const onPromptReady = vi.fn();
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={onEvaluateReadiness}
        onStartTask={onStartTask}
        onReadinessAction={vi.fn()}
        onCatalogChanged={vi.fn()}
        onPromptReady={onPromptReady}
        onClose={vi.fn()}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    fireEvent.click(screen.getByRole("button", { name: "Set up example" }));

    await waitFor(() =>
      expect(reconcileTutorialBlueprint).toHaveBeenCalledWith(
        "workspace-1",
        "network-case",
        "1.0.0",
        expect.any(String),
      ),
    );
    await screen.findByText("ready");
    fireEvent.click(screen.getByRole("button", { name: "Start task" }));

    await waitFor(() => expect(onStartTask).toHaveBeenCalledTimes(1));
    expect(
      (onStartTask.mock.calls[0] as unknown as unknown[])[3],
    ).toEqual(expect.any(String));
    expect(onPromptReady).toHaveBeenCalledWith(
      "Analyze the prepared network.",
    );
  });

  it("does not check readiness before the selected tutorial is installed", async () => {
    const onEvaluateReadiness = vi.fn(async () => ready);
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={onEvaluateReadiness}
        onStartTask={vi.fn()}
        onReadinessAction={vi.fn()}
        onCatalogChanged={vi.fn()}
        onPromptReady={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    const checkButton = screen.getByRole("button", { name: "Check readiness" });
    expect((checkButton as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(checkButton);
    expect(onEvaluateReadiness).not.toHaveBeenCalled();
    expect(
      (screen.getByRole("button", { name: "Start task" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it("keeps a partial installation incomplete and shows every typed issue", async () => {
    reconcileTutorialBlueprint.mockResolvedValueOnce({
      status: "partial",
      blueprint_id: blueprint.blueprint_id,
      revision: blueprint.revision,
      workspace_id: "workspace-1",
      dataset_release: null,
      agent_releases: [],
      supervisor_release: null,
      supervisor_policy: null,
      recommended_prompt: blueprint.recommended_prompt,
      expected_artifact_types: blueprint.expected_artifact_types,
      issues: [
        {
          code: "dataset_hash_mismatch",
          message: "The prepared customer data does not match this tutorial revision.",
        },
        {
          code: "supervisor_unavailable",
          message: "The governed Supervisor could not be published.",
        },
      ],
    });
    const onEvaluateReadiness = vi.fn(async () => ready);
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={onEvaluateReadiness}
        onStartTask={vi.fn()}
        onReadinessAction={vi.fn()}
        onCatalogChanged={vi.fn()}
        onPromptReady={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    fireEvent.click(screen.getByRole("button", { name: "Set up example" }));

    expect(
      await screen.findByText(
        "The prepared customer data does not match this tutorial revision.",
      ),
    ).toBeTruthy();
    expect(
      screen.getByText("The governed Supervisor could not be published."),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry setup" })).toBeTruthy();
    expect(
      screen.getByText("Install prepared resources").closest("li")
        ?.classList.contains("is-complete"),
    ).toBe(false);
    expect(
      (screen.getByRole("button", {
        name: "Check readiness",
      }) as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(onEvaluateReadiness).not.toHaveBeenCalled();
  });

  it("ignores a reconcile response after the dialog is closed", async () => {
    const pendingReconcile =
      deferred<Awaited<ReturnType<typeof reconcileTutorialBlueprint>>>();
    reconcileTutorialBlueprint.mockReturnValueOnce(pendingReconcile.promise);
    const onCatalogChanged = vi.fn();
    const onEvaluateReadiness = vi.fn(async () => ready);
    const onClose = vi.fn();
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={onEvaluateReadiness}
        onStartTask={vi.fn()}
        onReadinessAction={vi.fn()}
        onCatalogChanged={onCatalogChanged}
        onPromptReady={vi.fn()}
        onClose={onClose}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    fireEvent.click(screen.getByRole("button", { name: "Set up example" }));
    fireEvent.click(screen.getByRole("button", { name: "Close Learn" }));

    await act(async () => {
      pendingReconcile.resolve({
        status: "installed",
        blueprint_id: blueprint.blueprint_id,
        revision: blueprint.revision,
        workspace_id: "workspace-1",
        dataset_release: null,
        agent_releases: [],
        supervisor_release: null,
        supervisor_policy: {
          policy_id: "network-supervisor",
          version: "1.0.0",
        },
        recommended_prompt: blueprint.recommended_prompt,
        expected_artifact_types: blueprint.expected_artifact_types,
        issues: [],
      });
      await pendingReconcile.promise;
    });

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onCatalogChanged).not.toHaveBeenCalled();
    expect(onEvaluateReadiness).not.toHaveBeenCalled();
  });

  it("ignores a readiness response after the dialog is closed", async () => {
    const pendingReadiness = deferred<RunReadiness>();
    const onClose = vi.fn();
    const onEvaluateReadiness = vi.fn(() => pendingReadiness.promise);
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={onEvaluateReadiness}
        onStartTask={vi.fn()}
        onReadinessAction={vi.fn()}
        onCatalogChanged={vi.fn()}
        onPromptReady={vi.fn()}
        onClose={onClose}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    fireEvent.click(screen.getByRole("button", { name: "Set up example" }));
    await waitFor(() => expect(onEvaluateReadiness).toHaveBeenCalledTimes(1));
    fireEvent.click(screen.getByRole("button", { name: "Close Learn" }));

    await act(async () => {
      pendingReadiness.resolve(ready);
      await pendingReadiness.promise;
    });

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("ready")).toBeNull();
  });

  it("does not load a tutorial prompt when a pending start finishes after close", async () => {
    const pendingStart = deferred<boolean>();
    const onPromptReady = vi.fn();
    const onClose = vi.fn();
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={vi.fn(async () => ready)}
        onStartTask={vi.fn(() => pendingStart.promise)}
        onReadinessAction={vi.fn()}
        onCatalogChanged={vi.fn()}
        onPromptReady={onPromptReady}
        onClose={onClose}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    fireEvent.click(screen.getByRole("button", { name: "Set up example" }));
    await screen.findByText("ready");
    fireEvent.click(screen.getByRole("button", { name: "Start task" }));
    fireEvent.click(screen.getByRole("button", { name: "Close Learn" }));

    await act(async () => {
      pendingStart.resolve(true);
      await pendingStart.promise;
    });

    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onPromptReady).not.toHaveBeenCalled();
  });

  it("keeps an existing draft until the user explicitly replaces it", async () => {
    const onPromptReady = vi.fn()
      .mockReturnValueOnce(false)
      .mockReturnValueOnce(true);
    const onClose = vi.fn();
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={vi.fn(async () => ready)}
        onStartTask={vi.fn(async () => true)}
        onReadinessAction={vi.fn()}
        onCatalogChanged={vi.fn()}
        onPromptReady={onPromptReady}
        onClose={onClose}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    fireEvent.click(screen.getByRole("button", { name: "Set up example" }));
    await screen.findByText("ready");
    fireEvent.click(screen.getByRole("button", { name: "Start task" }));

    expect(
      await screen.findByText(
        "The task started and your existing draft was kept.",
      ),
    ).toBeTruthy();
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", {
        name: "Replace draft with tutorial prompt",
      }),
    );

    expect(onPromptReady).toHaveBeenLastCalledWith(
      blueprint.recommended_prompt,
      { replaceExisting: true },
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("keeps one operation identity when readiness changes before acceptance", async () => {
    const changed = Object.assign(new Error("readiness_changed"), {
      code: "readiness_changed",
    });
    const onStartTask = vi.fn()
      .mockRejectedValueOnce(changed)
      .mockResolvedValueOnce(true);
    const onEvaluateReadiness = vi.fn(async () => ready);
    render(
      <LearnDialog
        workspaces={[workspace]}
        activeWorkspaceId="workspace-1"
        busy={false}
        onEvaluateReadiness={onEvaluateReadiness}
        onStartTask={onStartTask}
        onReadinessAction={vi.fn()}
        onCatalogChanged={vi.fn()}
        onPromptReady={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    await screen.findByRole("heading", { name: "Prepared network case" });
    fireEvent.click(screen.getByRole("button", { name: "Set up example" }));
    await screen.findByText("ready");
    fireEvent.click(screen.getByRole("button", { name: "Start task" }));

    await waitFor(() => expect(onEvaluateReadiness).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(
        (screen.getByRole("button", {
          name: "Start task",
        }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole("button", { name: "Start task" }));

    await waitFor(() => expect(onStartTask).toHaveBeenCalledTimes(2));
    expect(onStartTask.mock.calls[0][3]).toBe(
      onStartTask.mock.calls[1][3],
    );
  });
});
