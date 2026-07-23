// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import MessageList from "./MessageList";

describe("MessageList", () => {
  afterEach(cleanup);

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

    expect(screen.getByText(/Checking whether/)).toBeTruthy();
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

  it("shows a resolved approval on the command card", () => {
    render(
      <MessageList
        items={[{
          id: "command-approved",
          level: "info",
          kind: "command_exec",
          text: "git push",
          toolStatus: "completed",
          cmdExitCode: 0,
          approvalStatus: "accepted",
        }]}
      />,
    );

    expect(screen.getByText("Accepted")).toBeTruthy();
    expect(screen.queryByText("Approval resolved")).toBeNull();
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

  it("renders tool calls as timeline siblings and drops empty reasoning wrappers", () => {
    const view = render(
      <MessageList
        items={[
          { id: "user-1", level: "user", text: "Check the weather" },
          { id: "reasoning-empty", level: "system", kind: "reasoning", text: "Reasoning completed" },
          { id: "search-1", level: "info", kind: "tool", text: "Web search", toolType: "Search", toolTitle: "weather Shenzhen", toolStatus: "completed" },
          { id: "final-1", level: "assistant", text: "It is sunny." },
        ]}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "1 tool call, 0 messages" }));
    const toolCard = screen.getByText("weather Shenzhen").closest(".web-tool-card");
    expect(toolCard).toBeTruthy();
    expect(toolCard?.closest(".web-reasoning")).toBeNull();
    expect(view.container.querySelector(".web-reasoning")).toBeNull();
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
    expect(view.container.querySelector(".web-execution-timeline")?.textContent).not.toContain("The project is rea");
    expect(view.container.querySelector(".web-execution-current")?.textContent).toContain("The project is rea");
    expect(within(view.container).getByRole("button", { name: "1 tool call, 3 messages" }).getAttribute("aria-expanded")).toBe("true");
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

    expect(screen.getByText("Inspecting files")).toBeTruthy();
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

  it("renders URL-mode MCP elicitation as key configuration instead of ordinary approval", () => {
    const onResolve = vi.fn();
    const view = render(
      <MessageList
        workspaceId="workspace-1"
        onResolveApproval={onResolve}
        thinking
        items={[
          {
            id: "user-url",
            level: "user",
            text: "Geocode the cities.",
          },
          {
            id: "approval-url",
            level: "info",
            kind: "approval",
            text: "Google Maps API key is not configured. Configure it in this app to save it globally and reuse it automatically.",
            approvalRequestId: "approval-url-1",
            approvalStatus: "pending",
            approvalMode: "url",
            approvalUrl: "http://127.0.0.1:43123/one-time-token",
            approvalServerName: "workspace_maps",
          },
        ]}
      />,
    );

    expect(screen.getByText("Map provider and API key required")).toBeTruthy();
    expect(screen.getByRole("button", { name: "配置 Key" })).toBeTruthy();
    expect(screen.queryByRole("link", { name: "Configure key" })).toBeNull();
    expect(screen.getByText("Waiting for API key…")).toBeTruthy();
    expect(screen.queryByText("Working…")).toBeNull();
    expect(within(view.container).queryByRole("button", { name: "Accept" })).toBeNull();
  });

  it("stops waiting for a map key after server-side delivery succeeds", () => {
    render(
      <MessageList
        thinking
        items={[
          {
            id: "user-mapbox",
            level: "user",
            text: "Geocode the cities.",
          },
          {
            id: "approval-mapbox-url",
            level: "info",
            kind: "approval",
            text: "Mapbox access token is not configured. Configure it in this app to save it globally and reuse it automatically.",
            approvalRequestId: "approval-mapbox-url-1",
            approvalStatus: "accepted",
            approvalMode: "url",
            approvalUrl: "http://127.0.0.1:43123/one-time-token",
            approvalServerName: "workspace_maps",
          },
          {
            id: "approval-next-tool",
            level: "info",
            kind: "approval",
            text: "Allow the workspace_maps MCP server to run tool \"batch_geocode\"?",
            approvalRequestId: "approval-next-tool-1",
            approvalStatus: "pending",
          },
        ]}
      />,
    );

    expect(screen.getByText("Waiting for approval…")).toBeTruthy();
    expect(screen.queryByText("Waiting for API key…")).toBeNull();
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
