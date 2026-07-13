// @vitest-environment jsdom
import { fireEvent, render, screen, within } from "@testing-library/react";
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

    expect(screen.getByText("running")).toBeTruthy();
  });

  it("renders a recoverable connection error as an active status", () => {
    const view = render(
      <MessageList
        items={[{
          id: "connection-1",
          level: "info",
          kind: "connection",
          text: "Connection interrupted. Reconnecting (1/5)…",
          streaming: true,
        }]}
      />,
    );

    expect(screen.getByRole("status").textContent).toContain("Reconnecting (1/5)");
    expect(view.container.querySelector(".web-thinking-spinner")).toBeTruthy();
  });

  it("groups turn activity and keeps the final answer outside the execution timeline", () => {
    const view = render(
      <MessageList
        items={[
          { id: "user-1", level: "user", text: "Inspect the project" },
          { id: "reasoning-1", level: "system", kind: "reasoning", text: "Reviewing project files" },
          { id: "commentary-1", level: "assistant", text: "I am checking the relevant files." },
          { id: "command-1", level: "info", kind: "command_exec", text: "rg --files", cmdExitCode: 0 },
          { id: "final-1", level: "assistant", text: "The project is ready." },
        ]}
      />,
    );

    const summary = screen.getByRole("button", { name: "1 tool call, 2 messages" });
    expect(summary.getAttribute("aria-expanded")).toBe("false");
    expect(view.container.querySelector(".web-execution-timeline")).toBeNull();
    fireEvent.click(summary);
    const timeline = view.container.querySelector(".web-execution-timeline");
    expect(timeline?.textContent).toContain("I am checking the relevant files.");
    expect(timeline?.textContent).not.toContain("The project is ready.");
  });

  it("renders live turn activity at the top level, then collapses it when the final answer completes", () => {
    const liveItems = [
      { id: "user-1", level: "user" as const, text: "Inspect the project" },
      { id: "reasoning-1", level: "system" as const, kind: "reasoning" as const, text: "Reviewing project files" },
      { id: "commentary-1", level: "assistant" as const, text: "I am checking the relevant files." },
      { id: "command-1", level: "info" as const, kind: "command_exec" as const, text: "rg --files", toolStatus: "running" },
      { id: "final-1", level: "assistant" as const, text: "The project is rea", streaming: true },
    ];
    const view = render(<MessageList items={liveItems} thinking />);

    expect(within(view.container).queryByText("1 tool call, 2 messages")).toBeNull();
    expect(view.container.querySelector(".web-execution-timeline")?.textContent).toContain("The project is rea");
    expect(screen.getByText("Working…")).toBeTruthy();

    view.rerender(
      <MessageList
        items={liveItems.map((item) => item.id === "command-1"
          ? { ...item, toolStatus: "completed" }
          : item.id === "final-1"
            ? { ...item, text: "The project is ready.", streaming: false }
            : item)}
        thinking={false}
      />,
    );

    const summary = within(view.container).getByRole("button", { name: "1 tool call, 2 messages" });
    expect(summary.getAttribute("aria-expanded")).toBe("false");
    expect(view.container.querySelector(".web-execution-timeline")).toBeNull();
    expect(within(view.container).getByText("The project is ready.")).toBeTruthy();
    expect(within(view.container).queryByText("Working…")).toBeNull();
  });

  it("passes the live reasoning state through to its collapsible block", () => {
    const view = render(
      <MessageList
        items={[{
          id: "reasoning-running",
          level: "system",
          kind: "reasoning",
          text: "Inspecting files",
          streaming: true,
        }]}
      />,
    );

    expect(screen.getAllByText("Inspecting files")).toHaveLength(2);
    expect(view.container.querySelector(".web-reasoning-working")).toBeTruthy();
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

  it("renders a resolved server request as non-interactive history", () => {
    const onResolve = vi.fn();
    const view = render(
      <MessageList
        workspaceId="workspace-1"
        onResolveApproval={onResolve}
        items={[{
          id: "approval-resolved",
          level: "info",
          kind: "approval",
          text: "git init",
          approvalRequestId: 43,
          approvalStatus: "resolved",
        }]}
      />,
    );

    expect(screen.getByText("Approval resolved")).toBeTruthy();
    expect(screen.getByText("Resolved")).toBeTruthy();
    expect(view.container.querySelector(".web-approval-accept")).toBeNull();
    expect(view.container.querySelector(".web-approval-deny")).toBeNull();
  });
});
