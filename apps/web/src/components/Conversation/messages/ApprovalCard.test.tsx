// @vitest-environment jsdom
import { fireEvent, render, screen } from "@testing-library/react";
import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import ApprovalCard from "./ApprovalCard";

describe("ApprovalCard", () => {
  afterEach(cleanup);

  it("opens one in-app provider dialog for map_utils URL elicitation", () => {
    const onResolve = vi.fn();
    render(
      <ApprovalCard
        command="A maps provider and API key are required. Configure Mapbox or Google in this app; the selected provider will be saved globally and reused."
        workspaceId="workspace-1"
        requestId="approval-1"
        mode="url"
        credentialKind="maps"
        onResolve={onResolve}
      />,
    );

    expect(screen.queryByRole("link", { name: "Configure key" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Accept" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "配置 Key" }));
    expect(
      screen.getByRole("dialog", { name: "配置地图服务 Key" }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "Mapbox" }).getAttribute("aria-pressed"))
      .toBe("true");
    expect(screen.getByRole("button", { name: "Google" })).toBeTruthy();
    expect(onResolve).not.toHaveBeenCalled();
  });

  it("keeps unsupported MCP credential requests unavailable", () => {
    const onResolve = vi.fn();
    render(
      <ApprovalCard
        command="Configure a key."
        workspaceId="workspace-1"
        requestId="approval-2"
        mode="url"
        onResolve={onResolve}
      />,
    );

    expect(screen.getByText(/not supported by the secure browser configuration flow/i)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onResolve).toHaveBeenCalledWith("workspace-1", "approval-2", "decline");
  });
});
