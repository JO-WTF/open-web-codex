// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Conversation from "./index";

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
  userInputRequest: null,
  submittingUserInput: false,
  onSubmitUserInput: vi.fn(),
  busy: false,
  sendDisabled: false,
};

describe("Conversation auto-scroll", () => {
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
