// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import Conversation from "./index";

afterEach(cleanup);

const baseProps = {
  goal: null,
  workspaceName: "workspace",
  threadTitle: null,
  conversationId: "thread-1",
  sidebarCollapsed: false,
  onToggleSidebar: vi.fn(),
  tokenUsage: null,
  threadStatus: "running",
  threadSettings: null,
  workspaceId: "workspace-1",
  thinking: false,
  draft: "",
  onDraftChange: vi.fn(),
  onSend: vi.fn(),
  onStop: vi.fn(),
  stopping: false,
  queuedFollowUps: [],
  steeringFollowUpId: null,
  canSteer: false,
  onSteerFollowUp: vi.fn(),
  onDeleteFollowUp: vi.fn(),
  pendingUserInputRequests: [],
  submittingPendingUserInputIds: new Set<string>(),
  onSubmitPendingUserInput: vi.fn(),
  pendingMcpFormRequests: [],
  submittingMcpFormIds: new Set<string>(),
  onSubmitMcpForm: vi.fn(),
  busy: false,
  sendDisabled: false,
};

describe("Conversation auto-scroll", () => {
  it("keeps a ready final artifact inside its producing turn when a later turn is added", () => {
    const view = render(
      <Conversation
        {...baseProps}
        messages={[
          { id: "user-1", level: "user", text: "生成简报。" },
          {
            id: "item-1",
            level: "info",
            kind: "tool",
            text: "publish report",
            toolType: "MCP",
            toolTitle: "supply_chain / publish_network_planning_report",
            toolStatus: "completed",
          },
          { id: "assistant-1", level: "assistant", text: "分析完成。" },
          { id: "user-2", level: "user", text: "继续解释成本。" },
          { id: "assistant-2", level: "assistant", text: "成本说明。" },
        ]}
        finalArtifacts={[{
          id: "artifact-1",
          task_id: "task-1",
          artifact_schema: "network_planning_report_markdown.v1",
          display_name: "Warehouse network planning report",
          mime_type: "text/markdown",
          expected_size: 1024,
          byte_size: 1024,
          content_sha256: "a".repeat(64),
          state: "ready",
          failure: null,
          content_url: "/api/artifacts/artifact-1/content",
          download_url: "/api/artifacts/artifact-1/download",
          producer_run_id: "run-1",
          producer_thread_id: "thread-1",
          producer_turn_id: "turn-1",
          producer_item_id: "item-1",
          producer_agent_role: "network_agent",
          created_at: "2026-08-11T00:00:00Z",
          updated_at: "2026-08-11T00:00:00Z",
        }]}
      />,
    );

    const link = screen.getByRole("link", { name: "下载 Markdown 文件" });
    expect(link.getAttribute("href")).toBe("/api/artifacts/artifact-1/download");
    expect(link.hasAttribute("download")).toBe(true);
    expect(screen.queryByText("Warehouse network planning report")).toBeNull();
    const text = view.container.textContent ?? "";
    expect(text.indexOf("分析完成。")).toBeLessThan(text.indexOf("下载 Markdown 文件"));
    expect(text.indexOf("下载 Markdown 文件")).toBeLessThan(text.indexOf("继续解释成本。"));
  });

  it("does not append another Thread's final artifact to the current conversation tail", () => {
    render(
      <Conversation
        {...baseProps}
        messages={[
          { id: "user-1", level: "user", text: "继续。" },
          { id: "assistant-1", level: "assistant", text: "新的回复。" },
        ]}
        finalArtifacts={[{
          id: "artifact-child",
          task_id: "task-1",
          artifact_schema: "network_planning_report_markdown.v1",
          display_name: "Child report",
          mime_type: "text/markdown",
          expected_size: 512,
          byte_size: 512,
          content_sha256: "b".repeat(64),
          state: "ready",
          failure: null,
          content_url: "/api/artifacts/artifact-child/content",
          download_url: "/api/artifacts/artifact-child/download",
          producer_run_id: "run-1",
          producer_thread_id: "child-thread",
          producer_turn_id: "child-turn",
          producer_item_id: "child-item",
          producer_agent_role: "network_agent",
          created_at: "2026-08-11T00:00:00Z",
          updated_at: "2026-08-11T00:00:00Z",
        }]}
      />,
    );

    expect(screen.queryByRole("link", { name: "下载 Markdown 文件" })).toBeNull();
  });

  it("does not show Working while the connection is reconnecting", () => {
    render(
      <Conversation
        {...baseProps}
        thinking
        threadStatus="reconnecting"
        messages={[]}
      />,
    );

    expect(screen.queryByText("Working…")).toBeNull();
  });

  it("still shows Working for an active turn", () => {
    render(
      <Conversation
        {...baseProps}
        thinking
        threadStatus="running"
        messages={[]}
      />,
    );

    expect(screen.getByText("Working…")).toBeTruthy();
  });

  it("keeps the goal panel visible without an active goal", () => {
    render(<Conversation {...baseProps} messages={[]} />);
    expect(screen.getByRole("button", { name: "No active goal" })).toBeTruthy();
  });

  it("follows streaming content growth while the user remains at the bottom", () => {
    const view = render(
      <Conversation
        {...baseProps}
        messages={[{ id: "assistant-1", level: "assistant", text: "Starting", streaming: true }]}
      />,
    );
    const area = view.container.querySelector<HTMLElement>(".web-message-area");
    expect(area).not.toBeNull();
    Object.defineProperty(area, "scrollHeight", { configurable: true, value: 900 });

    view.rerender(
      <Conversation
        {...baseProps}
        messages={[{
          id: "assistant-1",
          level: "assistant",
          text: "| City | Distance |\n| --- | --- |\n| Bekasi | 0 km |",
          streaming: true,
        }]}
      />,
    );

    expect(area?.scrollTop).toBe(900);
  });

  it("preserves the position after the user scrolls away from the bottom", () => {
    const view = render(
      <Conversation
        {...baseProps}
        messages={[{ id: "assistant-1", level: "assistant", text: "Starting", streaming: true }]}
      />,
    );
    const area = view.container.querySelector<HTMLElement>(".web-message-area");
    expect(area).not.toBeNull();
    Object.defineProperties(area, {
      scrollHeight: { configurable: true, value: 900 },
      clientHeight: { configurable: true, value: 400 },
    });
    if (area) {
      area.scrollTop = 100;
      fireEvent.scroll(area);
    }

    view.rerender(
      <Conversation
        {...baseProps}
        messages={[{ id: "assistant-1", level: "assistant", text: "More streamed content", streaming: true }]}
      />,
    );

    expect(area?.scrollTop).toBe(100);
  });
});
