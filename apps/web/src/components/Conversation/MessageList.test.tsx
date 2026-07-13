// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import MessageList from "./MessageList";

describe("MessageList", () => {
  it("renders reasoning and a restored command execution without message text", () => {
    render(
      <MessageList
        items={[
          {
            id: "reasoning-1",
            level: "system",
            kind: "reasoning",
            text: "Checking whether the workspace is already a repository.",
          },
          {
            id: "command-1",
            level: "info",
            kind: "command_exec",
            text: "/bin/zsh -lc 'git status'",
            toolStatus: "failed",
            cmdExitCode: 128,
            cmdOutput: "fatal: not a git repository",
          },
        ]}
      />,
    );

    expect(screen.getAllByText(/Checking whether/)).toHaveLength(2);
    expect(screen.getByText("git status")).toBeTruthy();
    expect(screen.getByText("✗ exit 128")).toBeTruthy();
    fireEvent.click(screen.getByText("git status"));
    expect(screen.getByText("fatal: not a git repository")).toBeTruthy();
  });

  it("renders a pending command as running", () => {
    render(
      <MessageList
        items={[
          {
            id: "command-running",
            level: "info",
            kind: "command_exec",
            text: "git init",
            toolStatus: "inProgress",
          },
        ]}
      />,
    );

    expect(screen.getByText("• running")).toBeTruthy();
  });

  it("passes workspace and thread context to approval decisions", () => {
    const onResolve = vi.fn();
    render(
      <MessageList
        workspaceId="workspace-1"
        onResolveApproval={onResolve}
        items={[
          {
            id: "approval-1",
            level: "info",
            kind: "approval",
            text: "git init",
            approvalRequestId: 42,
          },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "Accept" }));
    expect(onResolve).toHaveBeenCalledWith("workspace-1", 42, "accept");
  });
});
