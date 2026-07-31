// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type {
  AgentDefinitionSummary,
  RunReadiness,
} from "../../../browser/types";
import RunLauncherDialog from "./RunLauncherDialog";

const agent = (id: string, name: string): AgentDefinitionSummary => ({
  source: "user_release",
  release_id: `release-${id}`,
  definition_id: id,
  version: "1.0.0",
  display_name: name,
  description: `${name} description`,
  responsibilities: ["Analyze"],
  input_artifact_types: [],
  output_artifact_types: ["report.v1"],
  required_capabilities: [],
  capability_template: null,
  dataset_releases: [],
  required_workspace_id: null,
});

const ready = (fingerprint: string): RunReadiness => ({
  status: "ready",
  evaluation_fingerprint: fingerprint,
  checks: [],
});

function props(
  overrides: Partial<Parameters<typeof RunLauncherDialog>[0]> = {},
) {
  return {
    workspaceId: "workspace-1",
    workspaceName: "Demo",
    agents: [agent("one", "Agent One"), agent("two", "Agent Two")],
    agentsLoading: false,
    agentsError: null,
    supervisorPolicies: [],
    supervisorPoliciesLoading: false,
    supervisorPoliciesError: null,
    busy: false,
    onEvaluate: vi.fn(async () => ready("fingerprint-1")),
    onStart: vi.fn(async () => true),
    onAction: vi.fn(),
    onClose: vi.fn(),
    ...overrides,
  };
}

describe("RunLauncherDialog", () => {
  afterEach(cleanup);
  it("discards an older readiness response after the exact Release changes", async () => {
    let resolveFirst!: (value: RunReadiness) => void;
    let resolveSecond!: (value: RunReadiness) => void;
    const first = new Promise<RunReadiness>((resolve) => {
      resolveFirst = resolve;
    });
    const second = new Promise<RunReadiness>((resolve) => {
      resolveSecond = resolve;
    });
    const onEvaluate = vi
      .fn()
      .mockReturnValueOnce(first)
      .mockReturnValueOnce(second);
    render(<RunLauncherDialog {...props({ onEvaluate })} />);

    fireEvent.click(screen.getByText("Agent", { selector: "strong" }).closest("button")!);
    fireEvent.click(screen.getByRole("button", { name: /Agent One/ }));
    fireEvent.click(screen.getByRole("button", { name: /Agent Two/ }));
    resolveSecond(ready("newer"));
    await screen.findByText("Ready to start");

    resolveFirst({
      status: "blocked",
      evaluation_fingerprint: "older",
      checks: [{
        code: "workspace_dependencies",
        status: "blocked",
        message: "Old response",
        action: "open_workspace_data",
      }],
    });
    await Promise.resolve();
    expect(screen.queryByText("Old response")).toBeNull();
    expect(screen.getByText("Ready to start")).toBeTruthy();
  });

  it("allows a conversation Thread while exposing the typed data repair action", async () => {
    const onAction = vi.fn();
    render(
      <RunLauncherDialog
        {...props({
          onAction,
          onEvaluate: vi.fn(async (): Promise<RunReadiness> => ({
            // Conversation readiness is ready even when the downstream input
            // readiness is blocked.  Analysis uses a separate gate.
            status: "ready",
            scope: "thread",
            input_status: "blocked",
            evaluation_fingerprint: "thread-ready-input-blocked",
            checks: [{
              code: "data_intake",
              status: "blocked",
              message: "Publish the required Dataset Release.",
              action: "open_workspace_data",
            }],
          })),
        })}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /Standard/ }));
    await screen.findByText("Ready to start");
    const start = screen.getByRole("button", { name: "Create Thread" }) as HTMLButtonElement;
    expect(start.disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Add data" }));
    expect(onAction).toHaveBeenCalledWith("open_workspace_data");
  });

  it("rechecks automatically after a readiness fingerprint race", async () => {
    const onEvaluate = vi
      .fn()
      .mockResolvedValueOnce(ready("before"))
      .mockResolvedValueOnce(ready("after"));
    const changed = Object.assign(new Error("readiness_changed"), {
      code: "readiness_changed",
    });
    const onStart = vi.fn()
      .mockRejectedValueOnce(changed)
      .mockResolvedValueOnce(true);
    render(<RunLauncherDialog {...props({ onEvaluate, onStart })} />);

    fireEvent.click(screen.getByRole("button", { name: /Standard/ }));
    await screen.findByText("Ready to start");
    fireEvent.click(screen.getByRole("button", { name: "Create Thread" }));

    await waitFor(() => expect(onEvaluate).toHaveBeenCalledTimes(2));
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(screen.getByText("Ready to start")).toBeTruthy();
    await waitFor(() =>
      expect(
        (screen.getByRole("button", {
          name: "Create Thread",
        }) as HTMLButtonElement).disabled,
      ).toBe(false),
    );
    fireEvent.click(screen.getByRole("button", { name: "Create Thread" }));
    await waitFor(() => expect(onStart).toHaveBeenCalledTimes(2));
    expect(onStart.mock.calls[0][2]).toBe(onStart.mock.calls[1][2]);
  });

  it("locks duplicate starts and supports Escape", async () => {
    let resolveStart!: (value: boolean) => void;
    const onStart = vi.fn(
      () =>
        new Promise<boolean>((resolve) => {
          resolveStart = resolve;
        }),
    );
    const onClose = vi.fn();
    render(<RunLauncherDialog {...props({ onStart, onClose })} />);

    fireEvent.click(screen.getByRole("button", { name: /Standard/ }));
    await screen.findByText("Ready to start");
    const start = screen.getByRole("button", { name: "Create Thread" });
    fireEvent.click(start);
    fireEvent.click(start);
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(
      (screen.getByRole("button", { name: "Agent" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    resolveStart(false);
    await waitFor(() =>
      expect((start as HTMLButtonElement).disabled).toBe(false),
    );

    fireEvent.keyDown(
      screen.getByRole("dialog", { name: "Create Thread" }),
      { key: "Escape" },
    );
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("gives separate launcher instances different operation identities", async () => {
    const firstStart = vi.fn(async () => true);
    const first = render(
      <RunLauncherDialog {...props({ onStart: firstStart })} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Standard" }));
    await screen.findByText("Ready to start");
    fireEvent.click(screen.getByRole("button", { name: "Create Thread" }));
    await waitFor(() => expect(firstStart).toHaveBeenCalledTimes(1));
    const firstOperationId =
      (firstStart.mock.calls[0] as unknown as unknown[])[2];
    first.unmount();

    const secondStart = vi.fn(async () => true);
    render(<RunLauncherDialog {...props({ onStart: secondStart })} />);
    fireEvent.click(screen.getByRole("button", { name: "Standard" }));
    await screen.findByText("Ready to start");
    fireEvent.click(screen.getByRole("button", { name: "Create Thread" }));
    await waitFor(() => expect(secondStart).toHaveBeenCalledTimes(1));

    expect(firstOperationId).toEqual(expect.any(String));
    const secondOperationId =
      (secondStart.mock.calls[0] as unknown as unknown[])[2];
    expect(secondOperationId).toEqual(expect.any(String));
    expect(secondOperationId).not.toBe(firstOperationId);
  });
});
