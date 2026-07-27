// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import RightSidebar from "./RightSidebar";

afterEach(cleanup);

describe("RightSidebar", () => {
  it("keeps both panels mounted while displaying only the active tab", () => {
    const onTabChange = vi.fn();
    const view = render(
      <RightSidebar
        activeTab="files"
        agentsEnabled
        width={360}
        onWidthChange={vi.fn()}
        onTabChange={onTabChange}
        onClose={vi.fn()}
        agentPanel={<div data-testid="agent-content">Agent content</div>}
        filePanel={<input aria-label="File filter state" defaultValue="network" />}
      />,
    );

    expect(screen.getByRole("tabpanel", { name: "Workspace files" }).hasAttribute("hidden")).toBe(false);
    expect(screen.getByTestId("agent-content").closest("[role=tabpanel]")?.hasAttribute("hidden")).toBe(true);

    view.rerender(
      <RightSidebar
        activeTab="agents"
        agentsEnabled
        width={360}
        onWidthChange={vi.fn()}
        onTabChange={onTabChange}
        onClose={vi.fn()}
        agentPanel={<div data-testid="agent-content">Agent content</div>}
        filePanel={<input aria-label="File filter state" defaultValue="network" />}
      />,
    );

    expect(screen.getByRole("tabpanel", { name: "Agent activity" }).hasAttribute("hidden")).toBe(false);
    expect((screen.getByLabelText("File filter state") as HTMLInputElement).value).toBe("network");
  });

  it("disables Agent selection until a Runtime sub-agent exists", () => {
    const onTabChange = vi.fn();
    render(
      <RightSidebar
        activeTab="files"
        agentsEnabled={false}
        width={360}
        onWidthChange={vi.fn()}
        onTabChange={onTabChange}
        onClose={vi.fn()}
        agentPanel={<div />}
        filePanel={<div />}
      />,
    );

    const agents = screen.getByRole("tab", { name: "Agents" });
    expect((agents as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(agents);
    expect(onTabChange).not.toHaveBeenCalled();
  });
});
