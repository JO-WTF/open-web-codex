// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import Workspaces from "./Workspaces";

const workspace = {
  id: "ws-1",
  name: "Demo",
  path: "/tmp/demo",
  connected: true,
  settings: { sidebarCollapsed: false },
};

function baseProps() {
  return {
    workspaces: [workspace],
    activeId: "ws-1",
    onSelect: vi.fn(),
    onCreate: vi.fn(),
    onConnect: vi.fn(),
    onLoad: vi.fn(),
    busy: false,
    threadsByWorkspace: { "ws-1": [] },
    activeThreadId: null,
    onSelectThread: vi.fn(),
    onStartTask: vi.fn(),
    onArchiveThread: vi.fn(),
    onRemoveWorkspace: vi.fn(),
  };
}

describe("Web workspace actions", () => {
  afterEach(cleanup);
  it("places the remove action before the single New task action", () => {
    const props = baseProps();
    render(<Workspaces {...props} />);

    const remove = screen.getByRole("button", { name: "Remove workspace Demo" });
    const addThread = screen.getByRole("button", { name: "New task in Demo" });
    expect(
      remove.compareDocumentPosition(addThread) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(remove);
    expect(props.onRemoveWorkspace).toHaveBeenCalledWith("ws-1");
  });

  it("starts a Standard task directly in the selected Workspace", () => {
    const props = baseProps();
    render(<Workspaces {...props} />);

    fireEvent.click(screen.getByRole("button", { name: "New task in Demo" }));
    expect(props.onStartTask).toHaveBeenCalledWith("ws-1");
    expect(screen.queryByRole("dialog", { name: "Create Thread" })).toBeNull();
  });

  it("shows running state and confirms before archiving a thread", () => {
    const props = baseProps();
    const onArchiveThread = vi.fn();
    render(
      <Workspaces
        {...props}
        threadsByWorkspace={{
          "ws-1": [
            {
              id: "running-thread",
              label: "Running",
              updatedAt: 2,
              status: "running",
            },
            {
              id: "idle-thread",
              label: "Idle",
              updatedAt: 1,
              status: "idle",
            },
          ],
        }}
        onArchiveThread={onArchiveThread}
      />,
    );

    const runningIndicator = screen.getByRole("status", {
      name: "Thread is running",
    });
    const runningLabel = screen.getByText("Running");
    expect(
      runningLabel.compareDocumentPosition(runningIndicator) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(document.querySelector(".web-ws-thread-status.is-running")).toBeNull();
    expect(
      screen.getByText("Idle").parentElement?.querySelector(
        ".web-ws-thread-running",
      ),
    ).toBeNull();
    expect(
      (screen.getByRole("button", {
        name: "Archive thread Running",
      }) as HTMLButtonElement).disabled,
    ).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Archive thread Idle" }));
    expect(
      screen.getByRole("alertdialog", { name: "Archive thread?" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Archive" }));
    expect(onArchiveThread).toHaveBeenCalledWith("ws-1", "idle-thread");
  });
});
