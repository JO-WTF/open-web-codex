import { afterEach, describe, expect, it, vi } from "vitest";

import { CodexMonitorWebClient } from "./webClient";

describe("CodexMonitorWebClient.listThreadTurns", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("loads every full history page and returns chronological turns", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({
        result: { result: { data: [{ id: "turn-3" }, { id: "turn-2" }], nextCursor: "older" } },
      }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        result: { result: { data: [{ id: "turn-1" }], nextCursor: null } },
      }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);

    const client = new CodexMonitorWebClient({ baseUrl: "http://gateway.test" });
    await expect(client.listThreadTurns("workspace-1", "thread-1"))
      .resolves.toEqual([{ id: "turn-1" }, { id: "turn-2" }, { id: "turn-3" }]);

    const firstRequest = JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body));
    const secondRequest = JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body));
    expect(firstRequest).toMatchObject({
      method: "list_thread_turns",
      params: { workspaceId: "workspace-1", threadId: "thread-1" },
    });
    expect(secondRequest.params.cursor).toBe("older");
  });
});
