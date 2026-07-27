// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import Header from "./Header";

afterEach(cleanup);

describe("Header", () => {
  it("omits context usage and the Codex selector while keeping the Agent entry available", () => {
    render(
      <Header
        workspaceName="workspace"
        threadTitle={null}
        threadStatus="idle"
        sidebarCollapsed={false}
        onToggleSidebar={vi.fn()}
      />,
    );

    expect(screen.queryByTitle(/Context used/)).toBeNull();
    expect(screen.queryByTitle("Active coding agent")).toBeNull();
    expect((screen.getByLabelText("Agent activity") as HTMLButtonElement).disabled).toBe(false);
    expect(screen.getByLabelText("File manager")).toBeTruthy();
    const headerActions = screen.getAllByRole("button");
    expect(headerActions.indexOf(screen.getByLabelText("Agent activity")))
      .toBeLessThan(headerActions.indexOf(screen.getByLabelText("File manager")));
    expect(screen.queryByTitle("Terminal integration is not available in Web mode")).toBeNull();
    expect(screen.queryByTitle("Thread link")).toBeNull();
  });

  it("shows the thread name without exposing its id", () => {
    const { container } = render(
      <Header
        workspaceName="workspace"
        threadTitle="Generated title"
        threadStatus="idle"
        sidebarCollapsed={false}
        onToggleSidebar={vi.fn()}
      />,
    );

    expect(container.querySelector(".web-chat-workspace")?.textContent).toBe("workspace");
    expect(container.querySelector(".web-chat-title")?.textContent).toBe("Generated title");
    expect(container.textContent).not.toContain("CodexMonitor");
    expect(container.querySelectorAll(".web-chat-header-sep")).toHaveLength(1);
    expect(container.querySelector(".web-chat-workspace-chevron")).toBeNull();
  });

  it("enables the Agent entry and exposes unread activity", () => {
    render(
      <Header
        workspaceName="workspace"
        threadTitle="Plan"
        threadStatus="running"
        sidebarCollapsed={false}
        onToggleSidebar={vi.fn()}
        agentPanelAvailable
        agentPanelUnread
        rightPanelOpen
        activeRightPanelTab="agents"
        onOpenAgentPanel={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("Agent activity").getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByLabelText("New Agent activity")).toBeTruthy();
  });
});
