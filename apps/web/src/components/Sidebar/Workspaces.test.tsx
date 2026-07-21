// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Workspaces from "./Workspaces";

describe("Web workspace actions", () => {
  it("places the remove action before the new-thread action", () => {
    const onRemoveWorkspace = vi.fn();
    render(
      <Workspaces
        workspaces={[{
          id: "ws-1",
          name: "Demo",
          path: "/tmp/demo",
          connected: true,
          settings: { sidebarCollapsed: false },
        }]}
        activeId="ws-1"
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onConnect={vi.fn()}
        onLoad={vi.fn()}
        busy={false}
        threadsByWorkspace={{ "ws-1": [] }}
        activeThreadId={null}
        onSelectThread={vi.fn()}
        onNewThread={vi.fn()}
        onDeleteThread={vi.fn()}
        onRemoveWorkspace={onRemoveWorkspace}
      />,
    );

    const remove = screen.getByRole("button", { name: "Remove workspace Demo" });
    const addThread = screen.getByRole("button", { name: "New thread in Demo" });
    expect(remove.compareDocumentPosition(addThread) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    fireEvent.click(remove);
    expect(onRemoveWorkspace).toHaveBeenCalledWith("ws-1");
  });

  it("shows running state and archives a thread from its delete action", () => {
    const onDeleteThread = vi.fn();
    render(
      <Workspaces
        workspaces={[{
          id: "ws-1",
          name: "Demo",
          path: "/tmp/demo",
          connected: true,
          settings: { sidebarCollapsed: false },
        }]}
        activeId="ws-1"
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onConnect={vi.fn()}
        onLoad={vi.fn()}
        busy={false}
        threadsByWorkspace={{
          "ws-1": [
            { id: "running-thread", label: "Running", updatedAt: 2, status: "running" },
            { id: "idle-thread", label: "Idle", updatedAt: 1, status: "idle" },
          ],
        }}
        activeThreadId={null}
        onSelectThread={vi.fn()}
        onNewThread={vi.fn()}
        onDeleteThread={onDeleteThread}
        onRemoveWorkspace={vi.fn()}
      />,
    );

    expect(screen.getByLabelText("Running")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Delete thread Running" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Delete thread Idle" }));
    expect(onDeleteThread).toHaveBeenCalledWith("ws-1", "idle-thread");
  });
});
