import { afterEach, describe, expect, it, vi } from "vitest";

import { PlatformClient } from "./client";

describe("PlatformClient", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("uses typed Run and message routes without a generic RPC surface", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ run: { id: "run-1" } }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ status: "sent", thread_id: "thread-1" }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("crypto", { randomUUID: () => "018f-idempotency-key" });
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await client.startRun("task/one", "feature/safe");
    await client.sendMessage("task/one", "hello");

    expect(fetchMock.mock.calls[0]?.[0]).toBe("https://platform.test/api/tasks/task%2Fone/runs");
    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      idempotency_key: "018f-idempotency-key",
      git_ref: "feature/safe",
    });
    expect(fetchMock.mock.calls[1]?.[0]).toBe("https://platform.test/api/tasks/task%2Fone/messages");
    expect(fetchMock.mock.calls.every((call) => !String(call[0]).includes("/api/rpc"))).toBe(true);
  });

  it("authenticates WebSocket in the first frame instead of putting the token in its URL", () => {
    const instances: FakeSocket[] = [];
    class FakeSocket {
      onopen: (() => void) | null = null;
      onmessage: ((message: { data: string }) => void) | null = null;
      onclose: (() => void) | null = null;
      onerror: (() => void) | null = null;
      sent: string[] = [];
      constructor(readonly url: string | URL) {
        instances.push(this);
      }
      send(value: string) { this.sent.push(value); }
      close() {}
    }
    vi.stubGlobal("WebSocket", FakeSocket);
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "secret-session" });
    const unsubscribe = client.subscribe(() => undefined, () => undefined);
    const socket = instances[0];
    expect(String(socket?.url)).toBe("wss://platform.test/api/events/ws");
    expect(String(socket?.url)).not.toContain("secret-session");
    socket?.onopen?.();
    expect(JSON.parse(socket?.sent[0] ?? "{}")).toEqual({
      type: "authenticate",
      token: "secret-session",
    });
    unsubscribe();
  });
});
