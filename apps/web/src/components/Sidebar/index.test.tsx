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

  it("renders Codex quota windows above the settings control", () => {
    const view = render(
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
        rateLimits={{
          primary: { usedPercent: 12, resetsAt: Date.now() + 3_600_000 },
          secondary: { usedPercent: 67, resetsAt: Date.now() + 86_400_000 },
          credits: { hasCredits: true, unlimited: false, balance: "40" },
        }}
        busy={false}
      />,
    );

    expect(screen.getByLabelText("Codex usage limits")).toBeTruthy();
    expect(screen.getByText("Session")).toBeTruthy();
    expect(screen.getByText("Weekly")).toBeTruthy();
    expect(screen.getByText("Credits: 40 credits")).toBeTruthy();
    const bottom = view.container.querySelector(".web-sidebar-bottom");
    expect(bottom?.firstElementChild?.classList.contains("web-quota-card")).toBe(true);
  });
});
