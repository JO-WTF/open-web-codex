import { afterEach, describe, expect, it, vi } from "vitest";

import { PlatformClient } from "./platformClient";

function mockFetch(handler: (url: string, init?: RequestInit) => Response | Promise<Response>) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    return handler(url, init);
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("PlatformClient.health", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("requests /api/health", async () => {
    const fetchMock = mockFetch(() =>
      new Response(JSON.stringify({ ok: true, version: "1.0.0" }), { status: 200 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test" });
    await expect(client.health()).resolves.toEqual({ ok: true, version: "1.0.0" });
    expect(fetchMock).toHaveBeenCalledWith("http://platform.test/api/health", expect.any(Object));
  });
});

describe("PlatformClient.auth", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("logs in via /api/sessions", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/sessions");
      expect(init?.method).toBe("POST");
      expect(JSON.parse(String(init?.body))).toEqual({ email: "a@b.com", password: "secret" });
      return new Response(JSON.stringify({ session_token: "tok", user: { id: "u1" } }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test" });
    await client.login("a@b.com", "secret");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("bootstraps via /api/bootstrap", async () => {
    mockFetch(() =>
      new Response(JSON.stringify({ session_token: "tok", user: { id: "u1" } }), { status: 200 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test" });
    await client.bootstrap("Admin", "a@b.com", "secret");
  });

  it("loads /api/me with bearer token", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/me");
      expect(init?.headers).toMatchObject({ authorization: "Bearer session" });
      return new Response(JSON.stringify({ id: "u1", email: "a@b.com" }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "session" });
    await client.me();
    expect(fetchMock).toHaveBeenCalled();
  });
});

describe("PlatformClient.projects", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("lists projects", async () => {
    mockFetch(() => new Response(JSON.stringify([]), { status: 200 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.listProjects();
  });

  it("creates a project", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/projects");
      expect(JSON.parse(String(init?.body))).toEqual({
        name: "demo",
        git_url: "https://github.com/example/repo.git",
      });
      return new Response(JSON.stringify({ id: "p1" }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.createProject({ name: "demo", git_url: "https://github.com/example/repo.git" });
    expect(fetchMock).toHaveBeenCalled();
  });

  it("deletes a project", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/projects/p1");
      expect(init?.method).toBe("DELETE");
      return new Response(JSON.stringify({ deleted: true, id: "p1" }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.deleteProject("p1");
    expect(fetchMock).toHaveBeenCalled();
  });
});

describe("PlatformClient.tasks and runs", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("lists tasks for a project", async () => {
    const fetchMock = mockFetch((url) => {
      expect(url).toBe("http://platform.test/api/tasks?project_id=proj%2F1");
      return new Response(JSON.stringify([]), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.listTasks("proj/1");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("starts a task run with idempotency key", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/tasks/task-1/runs");
      expect(init?.headers).toMatchObject({ "idempotency-key": "idem" });
      return new Response(JSON.stringify({ run: { id: "run-1" } }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.startTaskRun("task-1", "idem");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("loads active run", async () => {
    mockFetch(() => new Response(JSON.stringify({ run: null }), { status: 200 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.getActiveRun("task-1");
  });

  it("sends a message", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/tasks/task-1/messages");
      expect(JSON.parse(String(init?.body))).toEqual({ text: "hello" });
      return new Response(JSON.stringify({ status: "sent", thread_id: "th-1" }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.sendMessage("task-1", "hello");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("polls task events after sequence", async () => {
    const fetchMock = mockFetch((url) => {
      expect(url).toBe("http://platform.test/api/tasks/task%2F1/events?limit=200&after_sequence=42");
      return new Response(JSON.stringify([]), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.listTaskEvents("task/1", 42);
    expect(fetchMock).toHaveBeenCalled();
  });
});

describe("PlatformClient.approvals", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("lists approvals for a run", async () => {
    const fetchMock = mockFetch((url) => {
      expect(url).toBe("http://platform.test/api/approvals?run_id=run-1");
      return new Response(JSON.stringify([]), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.listApprovals("run-1");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("decides an approval", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/approvals/appr-1/decision");
      expect(JSON.parse(String(init?.body))).toEqual({ decision: "approved" });
      return new Response(JSON.stringify({ approval: { id: "appr-1" } }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.decideApproval("appr-1", "approved");
    expect(fetchMock).toHaveBeenCalled();
  });
});

describe("PlatformClient.run controls and files", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("interrupts a turn", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/runs/run-1/interrupt");
      expect(JSON.parse(String(init?.body))).toEqual({ turn_id: "turn-1" });
      return new Response(JSON.stringify({ status: "interrupted" }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.interruptRun("run-1", "turn-1");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("steers a turn", async () => {
    const fetchMock = mockFetch((url, init) => {
      expect(url).toBe("http://platform.test/api/runs/run-1/steer");
      expect(JSON.parse(String(init?.body))).toEqual({ turn_id: "turn-1", text: "focus on tests" });
      return new Response(JSON.stringify({ status: "steered" }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.steerRun("run-1", "turn-1", "focus on tests");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("lists run files", async () => {
    mockFetch(() => new Response(JSON.stringify({ files: ["README.md"] }), { status: 200 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await expect(client.listRunFiles("run-1")).resolves.toEqual({ files: ["README.md"] });
  });

  it("reads run file content", async () => {
    const fetchMock = mockFetch((url) => {
      expect(url).toBe("http://platform.test/api/runs/run-1/files/content?path=README.md");
      return new Response(JSON.stringify({ path: "README.md", content: "hi", truncated: false }), { status: 200 });
    });
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.readRunFile("run-1", "README.md");
    expect(fetchMock).toHaveBeenCalled();
  });

  it("loads git status", async () => {
    mockFetch(() => new Response(JSON.stringify({ files: [{ path: "a.ts", status: "M" }] }), { status: 200 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await client.getRunGitStatus("run-1");
  });
});

describe("PlatformClient errors", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("throws platform error messages from PlatformError body", async () => {
    mockFetch(() =>
      new Response(JSON.stringify({
        kind: "forbidden",
        message: "organization access denied",
        request_id: null,
      }), { status: 403 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await expect(client.me()).rejects.toThrow("organization access denied");
  });

  it("falls back to generic message for empty bodies", async () => {
    mockFetch(() => new Response("", { status: 500 }));
    const client = new PlatformClient({ baseUrl: "http://platform.test", token: "t" });
    await expect(client.me()).rejects.toThrow("Platform request failed (HTTP 500).");
  });
});
