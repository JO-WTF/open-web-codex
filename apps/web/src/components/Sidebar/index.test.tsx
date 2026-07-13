// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import Sidebar from "./index";

describe("Sidebar settings", () => {
  it("opens settings in a dialog and closes it with Escape", () => {
    render(
      <Sidebar
        gatewayState="online"
        gatewayVersion="1.0.0"
        workspaces={[]}
        activeWorkspaceId={null}
        onSelectWorkspace={vi.fn()}
        onCreateWorkspace={vi.fn()}
        onLoadWorkspaces={vi.fn()}
        onConnectWorkspace={vi.fn()}
        threadsByWorkspace={{}}
        activeThreadId={null}
        onSelectThread={vi.fn()}
        onNewThread={vi.fn()}
        baseUrl="http://127.0.0.1:4733"
        token=""
        onBaseUrlChange={vi.fn()}
        onTokenChange={vi.fn()}
        onCheckGateway={vi.fn()}
        mcpServers={{}}
        rateLimits={null}
        busy={false}
      />,
    );

    expect(screen.queryByRole("dialog", { name: "Settings" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open settings" }));
    expect(screen.getByRole("dialog", { name: "Settings" })).toBeTruthy();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Settings" })).toBeNull();
  });
});
