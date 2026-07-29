// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import PythonCapabilityEditor from "./PythonCapabilityEditor";

const {
  validatePythonCapability,
  testPythonCapability,
  publishPythonCapability,
} = vi.hoisted(() => ({
  validatePythonCapability: vi.fn(),
  testPythonCapability: vi.fn(),
  publishPythonCapability: vi.fn(),
}));

vi.mock("../../../browser/session", () => ({
  platformClient: {
    validatePythonCapability,
    testPythonCapability,
    publishPythonCapability,
  },
}));

const workspace = {
  id: "workspace-1",
  name: "Stocks",
  path: "/workspace/stocks",
  connected: true,
  settings: { sidebarCollapsed: false },
};

describe("PythonCapabilityEditor", () => {
  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("validates the generated stock Tool and Skill contract", async () => {
    validatePythonCapability.mockResolvedValue({
      valid: true,
      tool_names: ["lookup_stock", "get_price_history"],
      issues: [],
    });
    render(
      <PythonCapabilityEditor
        workspaces={[workspace]}
        activeWorkspaceId={workspace.id}
        onPublished={vi.fn()}
        onStartThread={vi.fn()}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Validate" }));

    await waitFor(() => expect(validatePythonCapability).toHaveBeenCalledTimes(1));
    expect(validatePythonCapability.mock.calls[0][0]).toBe(workspace.id);
    expect(validatePythonCapability.mock.calls[0][1]).toMatchObject({
      slug: "stock-history",
      server_name: "stock_data",
      tools: [
        { name: "lookup_stock" },
        { name: "get_price_history" },
      ],
      skill: { name: "stock-history" },
    });
    expect(
      screen.getByText(/MCP startup and Tool discovery passed/),
    ).toBeTruthy();
  });

  it("publishes once and starts a Thread in the selected Workspace", async () => {
    const onPublished = vi.fn();
    const onStartThread = vi.fn();
    publishPythonCapability.mockResolvedValue({
      release_id: "release-1",
      package_id: "stock-history",
      version: "1.0.0",
      capability_root_id: "local-stock-history-1-0-0",
      server_name: "stock_data",
      skill_name: "stock-history",
      content_sha256: "a".repeat(64),
      written_files: [],
    });
    render(
      <PythonCapabilityEditor
        workspaces={[workspace]}
        activeWorkspaceId={workspace.id}
        onPublished={onPublished}
        onStartThread={onStartThread}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Publish package" }));
    await waitFor(() => expect(publishPythonCapability).toHaveBeenCalledTimes(1));
    expect(onPublished).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Start test Thread" }));
    expect(onStartThread).toHaveBeenCalledWith(workspace.id);
  });
});
