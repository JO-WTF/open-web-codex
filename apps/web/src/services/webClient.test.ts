import { afterEach, describe, expect, it, vi } from "vitest";

import { CodexMonitorWebClient } from "./webClient";
import { registerBrowserCommand } from "@/platform/browser/core";

const unregisterCommands: Array<() => void> = [];

function mockCommand(
  name: string,
  handler: (payload: Record<string, unknown>) => unknown | Promise<unknown>,
) {
  const mock = vi.fn(handler);
  unregisterCommands.push(registerBrowserCommand(name, async (payload) => await mock(payload)));
  return mock;
}

afterEach(() => {
  unregisterCommands.splice(0).forEach((unregister) => unregister());
  vi.unstubAllGlobals();
});

describe("CodexMonitorWebClient threads", () => {
  it("loads every historical thread page", async () => {
    const command = mockCommand("list_threads", (payload) => payload.cursor
      ? { data: [{ id: "thread-1" }], nextCursor: null }
      : { data: [{ id: "thread-2" }], nextCursor: "older" });

    const client = new CodexMonitorWebClient({ baseUrl: "http://gateway.test" });
    await expect(client.listThreads("workspace-1")).resolves.toEqual({
      data: [{ id: "thread-2" }, { id: "thread-1" }],
    });
    expect(command).toHaveBeenCalledTimes(2);
  });

  it("passes the selected model on every message turn and archives deleted threads", async () => {
    const send = mockCommand("send_user_message", () => ({}));
    const archive = mockCommand("archive_thread", () => ({}));
    const client = new CodexMonitorWebClient({ baseUrl: "http://gateway.test" });

    await client.sendUserMessage("workspace-1", "thread-1", "hello", "gpt-next");
    await client.archiveThread("workspace-1", "thread-1");

    expect(send).toHaveBeenCalledWith(expect.objectContaining({
      workspaceId: "workspace-1",
      threadId: "thread-1",
      model: "gpt-next",
      text: "hello",
      accessMode: "current",
    }));
    expect(archive).toHaveBeenCalledWith({ workspaceId: "workspace-1", threadId: "thread-1" });
  });
});

describe("CodexMonitorWebClient.listThreadTurns", () => {
  it("loads every full history page and returns chronological turns", async () => {
    const command = mockCommand("list_thread_turns", (payload) => payload.cursor
      ? { data: [{ id: "turn-1" }], nextCursor: null }
      : { data: [{ id: "turn-3" }, { id: "turn-2" }], nextCursor: "older" });

    const client = new CodexMonitorWebClient({ baseUrl: "http://gateway.test" });
    await expect(client.listThreadTurns("workspace-1", "thread-1"))
      .resolves.toEqual([{ id: "turn-1" }, { id: "turn-2" }, { id: "turn-3" }]);

    expect(command.mock.calls[0]?.[0]).toMatchObject({
      workspaceId: "workspace-1",
      threadId: "thread-1",
    });
    expect(command.mock.calls[1]?.[0]?.cursor).toBe("older");
  });
});

describe("CodexMonitorWebClient workspace status snapshots", () => {
  it("requests MCP and account usage snapshots for the active workspace", async () => {
    const mcp = mockCommand("list_mcp_server_status", () => ({}));
    const rateLimits = mockCommand("account_rate_limits", () => ({}));

    const client = new CodexMonitorWebClient({ baseUrl: "http://gateway.test" });
    await Promise.all([
      client.listMcpServerStatus("workspace-1"),
      client.getAccountRateLimits("workspace-1"),
    ]);

    expect(mcp).toHaveBeenCalledWith({ workspaceId: "workspace-1", limit: 100 });
    expect(rateLimits).toHaveBeenCalledWith({ workspaceId: "workspace-1" });
  });
});

describe("CodexMonitorWebClient.listTaskEvents", () => {
  it("requests monotonic replay after the last persisted sequence", async () => {
    const fetchMock = vi.fn(async () =>
      new Response(JSON.stringify([]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const client = new CodexMonitorWebClient({ baseUrl: "http://platform.test" });
    await client.listTaskEvents("task/one", 42, 100);

    expect(fetchMock).toHaveBeenCalledWith(
      "http://platform.test/api/tasks/task%2Fone/events?limit=100&after_sequence=42",
      expect.objectContaining({ headers: {} }),
    );
  });
});
