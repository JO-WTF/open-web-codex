import { describe, expect, it } from "vitest";

import { summarizeWebAppServerEvent } from "./webAppServerEventSummary";

describe("summarizeWebAppServerEvent", () => {
  it("formats workspace connection notifications", () => {
    expect(summarizeWebAppServerEvent({
      workspace_id: "workspace-1",
      message: {
        method: "codex/connected",
        params: { workspaceId: "workspace-1" },
      },
    })).toBe("Workspace connected");
  });

  it("formats remote control status notifications without raw identifiers", () => {
    expect(summarizeWebAppServerEvent({
      workspace_id: "workspace-1",
      message: {
        method: "remoteControl/status/changed",
        params: {
          environmentId: null,
          installationId: "installation-secret",
          serverName: "ZhongdeMacBook-Air.local",
          status: "disabled",
        },
      },
    })).toBe("Remote control disabled · ZhongdeMacBook-Air.local");
  });
});
