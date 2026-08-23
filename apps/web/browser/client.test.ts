import { afterEach, describe, expect, it, vi } from "vitest";

import {
  isPlatformRequestError,
  PlatformClient,
} from "./client";

describe("PlatformClient", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("reads only bounded native inline visualization files", async () => {
    const threadId = "0198ff2f-82ee-7cc9-a3e6-2974debf8666";
    const fetchMock = vi.fn().mockResolvedValue(new Response("image", {
      status: 200,
      headers: { "content-type": "image/png" },
    }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    const result = await client.readInlineVisualization(threadId, "chart.png");

    expect(result.contentType).toBe("image/png");
    expect(fetchMock).toHaveBeenCalledWith(
      `https://platform.test/api/threads/${threadId}/inline-visualizations/chart.png`,
      expect.objectContaining({
        cache: "no-store",
        headers: { authorization: "Bearer session-token" },
      }),
    );
    await expect(client.readInlineVisualization(threadId, "unsafe.svg"))
      .rejects.toThrow("file is invalid");
  });

  it("rejects multi-file Workspace uploads before issuing a request", () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    expect(() => client.uploadWorkspaceFiles("workspace-1", [
      new File(["first"], "first.csv"),
      new File(["second"], "second.csv"),
    ])).toThrow("Upload exactly one Workspace file per request.");
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("binds a new Task to one Workspace and one selected Copilot package", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ id: "task-1", workspace_id: "workspace-1" }), {
        status: 200,
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await client.createTask(
      "project-1",
      "workspace-1",
      "Plan the network",
      null,
      { packageId: "warehouse-network-single-agent" },
    );

    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      project_id: "project-1",
      workspace_id: "workspace-1",
      title: "Plan the network",
      model_provider: null,
      model: null,
      copilot_package_id: "warehouse-network-single-agent",
    });
  });

  it("activates and deactivates Copilots through package-id-only REST calls", async () => {
    const status = { packages: [], installations: [] };
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(status), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(status), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await client.activateCopilot("meeting-action-review");
    await client.deactivateCopilot("meeting-action-review");

    expect(fetchMock.mock.calls.map(([url, init]) => ({
      url,
      method: init?.method,
      body: JSON.parse(String(init?.body)),
      authorization: (init?.headers as Record<string, string>)?.authorization,
    }))).toEqual([
      {
        url: "https://platform.test/api/profile/copilots/activate",
        method: "POST",
        body: { packageId: "meeting-action-review" },
        authorization: "Bearer session-token",
      },
      {
        url: "https://platform.test/api/profile/copilots/deactivate",
        method: "POST",
        body: { packageId: "meeting-action-review" },
        authorization: "Bearer session-token",
      },
    ]);
  });

  it("reads and answers run-scoped MCP forms through the dedicated contract", async () => {
    const pending = [{
      id: "approval-1",
      runId: "run-1",
      source: { kind: "agent", executionId: "execution-1", displayTitle: "Data Agent" },
      serverName: "supply_chain_data",
      message: "Provide inputs",
      fields: [],
      state: "pending",
      version: 2,
      createdAt: "2026-08-09T00:00:00Z",
    }];
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(pending), { status: 200 }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await expect(client.listRunMcpFormRequests("run/one")).resolves.toEqual(pending);
    await client.respondMcpForm(
      "approval/one",
      "accept",
      { factor: 1.35, method: "navigation" },
      2,
    );

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/runs/run%2Fone/mcp-form-requests",
    );
    expect(fetchMock.mock.calls[1]?.[0]).toBe(
      "https://platform.test/api/approvals/approval%2Fone/mcp-form",
    );
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toEqual({
      action: "accept",
      content: { factor: 1.35, method: "navigation" },
      version: 2,
    });
  });

  it("omits untouched MCP form content while preserving explicit false and empty-array values", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(null, { status: 204 }))
      .mockResolvedValueOnce(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await client.respondMcpForm("approval-omit", "accept", undefined, 2);
    await client.respondMcpForm(
      "approval-explicit",
      "accept",
      { includeExistingWarehouses: false, candidateTiers: [] },
      2,
    );

    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      action: "accept",
      version: 2,
    });
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toEqual({
      action: "accept",
      content: { includeExistingWarehouses: false, candidateTiers: [] },
      version: 2,
    });
  });

  it("uses typed Run and message routes without a generic RPC surface", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ run: { id: "run-1" } }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        status: "sent",
        thread_id: "thread-1",
        turn_id: "turn-1",
        thread_name: "hello",
      }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("crypto", { randomUUID: () => "018f-idempotency-key" });
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await client.startRun("task/one", {});
    await expect(client.sendMessage("task/one", "hello", {
      model: "deepseek-v4-flash",
      modelProvider: "deepseek",
    })).resolves.toMatchObject({
      thread_id: "thread-1",
      turn_id: "turn-1",
      thread_name: "hello",
    });

    expect(fetchMock.mock.calls[0]?.[0]).toBe("https://platform.test/api/tasks/task%2Fone/runs");
    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      idempotency_key: "018f-idempotency-key",
      fork_thread_id: null,
      fork_source_run_id: null,
    });
    expect(fetchMock.mock.calls[1]?.[0]).toBe("https://platform.test/api/tasks/task%2Fone/messages");
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toMatchObject({
      text: "hello",
      model: "deepseek-v4-flash",
      model_provider: "deepseek",
    });
    expect(fetchMock.mock.calls.every((call) => !String(call[0]).includes("/api/rpc"))).toBe(true);
  });

  it("reads persisted Agent task executions through the typed Run resource", async () => {
    const executions = [{
      id: "execution-1",
      run_id: "run-1",
      thread_id: "child-thread",
      turn_id: "turn-1",
      ordinal: 1,
      task: "Validate inputs",
      status: "completed",
      current_behavior: "Finished this work cycle",
      latest_progress: "Inputs validated",
      first_observed_sequence: 4,
      last_observed_sequence: 9,
      started_at: "2026-07-27T00:00:00Z",
      completed_at: "2026-07-27T00:00:01Z",
      created_at: "2026-07-27T00:00:00Z",
      updated_at: "2026-07-27T00:00:01Z",
    }];
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify(executions), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.listRunAgentExecutions("run/one")).resolves.toEqual(executions);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://platform.test/api/runs/run%2Fone/agent-executions",
      expect.objectContaining({ cache: "no-store" }),
    );
  });

  it("reads only Run-scoped Agent history and authorized Artifact content", async () => {
    const turns = [{
      id: "turn-2",
      status: "completed",
      items: [{ id: "message-2", type: "agentMessage", text: "Follow-up complete" }],
    }];
    const artifact = { schema_version: "planning-dataset.v1" };
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(turns), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(artifact), {
        status: 200,
        headers: { "content-type": "application/json" },
      }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8666";

    await expect(
      client.listRunAgentThreadTurns("run/one", "agent/one"),
    ).resolves.toEqual(turns);
    await expect(client.readArtifactContent(artifactId)).resolves.toEqual({
      kind: "json",
      mime_type: "application/json",
      value: artifact,
    });

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/runs/run%2Fone/agents/agent%2Fone/turns",
    );
    expect(fetchMock.mock.calls[1]?.[0]).toBe(
      `https://platform.test/api/artifacts/${artifactId}/content`,
    );
  });

  it("reads an authorized Markdown Artifact without JSON coercion", async () => {
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8667";
    const markdown = "# Warehouse network planning report\n\nReadable brief.\n";
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(markdown, {
        status: 200,
        headers: { "content-type": "text/markdown; charset=utf-8" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.readArtifactContent(artifactId)).resolves.toEqual({
      kind: "markdown",
      mime_type: "text/markdown",
      text: markdown,
    });
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      `https://platform.test/api/artifacts/${artifactId}/content`,
    );
  });

  it("downloads an authorized Markdown Artifact with its attachment filename", async () => {
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8668";
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response("# Warehouse network planning report\n", {
        status: 200,
        headers: {
          "content-type": "text/markdown; charset=utf-8",
          "content-disposition": "attachment; filename=\"network-brief.md\"",
        },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    const downloaded = await client.downloadArtifact(artifactId);

    expect(downloaded.filename).toBe("network-brief.md");
    expect(await downloaded.blob.text()).toBe("# Warehouse network planning report\n");
    expect(fetchMock).toHaveBeenCalledWith(
      `https://platform.test/api/artifacts/${artifactId}/download`,
      expect.objectContaining({
        cache: "no-store",
        headers: { authorization: "Bearer session-token" },
      }),
    );
  });

  it("sends only the exact fork source when forking a Standard Run", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ run: { id: "run-1" } }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("crypto", { randomUUID: () => "018f-idempotency-key" });
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await client.startRun("task-one", {
      forkThreadId: "thread-source",
      forkSourceRunId: "run-source",
    });

    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      idempotency_key: "018f-idempotency-key",
      fork_thread_id: "thread-source",
      fork_source_run_id: "run-source",
    });
  });

  it("loads the read-only Runtime Agent projection for a Run", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify([{ thread_id: "thread-1", is_root: true }]), {
        status: 200,
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.listRunAgents("run/one")).resolves.toEqual([
      { thread_id: "thread-1", is_root: true },
    ]);
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/runs/run%2Fone/agents",
    );
    expect(fetchMock.mock.calls[0]?.[1]?.method).toBeUndefined();
  });

  it("loads independently authorized Artifacts for a Task", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify([{
        id: "artifact-1",
        task_id: "task/one",
        artifact_schema: "planning-dataset.v1",
        state: "ready",
      }]), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.listTaskArtifacts("task/one")).resolves.toEqual([
      expect.objectContaining({
        id: "artifact-1",
        artifact_schema: "planning-dataset.v1",
      }),
    ]);
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/tasks/task%2Fone/artifacts",
    );
  });

  it("reads map source content through the stable authorized Artifact resource", async () => {
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8666";
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ type: "FeatureCollection", features: [] }), {
        status: 200,
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(
      client.readReplyArtifact(`/api/artifacts/${artifactId}/content`),
    ).resolves.toEqual({ type: "FeatureCollection", features: [] });
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      `https://platform.test/api/artifacts/${artifactId}/content`,
    );
  });

  it("reads a bounded inline-map source through its authorized Run projection", async () => {
    const runId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8666";
    const path = `/api/runs/${runId}/inline-maps/map-network/sources/network`;
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ type: "FeatureCollection", features: [] }), {
        status: 200,
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.readReplyArtifact(path)).resolves.toEqual({
      type: "FeatureCollection",
      features: [],
    });
    expect(fetchMock.mock.calls[0]?.[0]).toBe(`https://platform.test${path}`);
  });

  it("rejects an obsolete Run-scoped Artifact path", async () => {
    const client = new PlatformClient({ baseUrl: "https://platform.test" });

    await expect(
      client.readReplyArtifact(
        "/api/runs/8e98ff2f-82ee-4cc9-a3e6-2974debf8666/artifacts/8e98ff2f-82ee-4cc9-a3e6-2974debf8666",
      ),
    ).rejects.toThrow("Reply Artifact path is invalid.");
  });

  it("falls back when crypto.randomUUID is unavailable", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ run: { id: "run-1" } }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("crypto", {});
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await client.startRun("task-one", {});

    const body = JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body));
    expect(body.idempotency_key).toMatch(/^idempotency-/);
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

  it("paginates the complete durable Task event history by sequence", async () => {
    const firstPage = Array.from({ length: 200 }, (_, index) => ({ sequence: index + 1 }));
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify(firstPage), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify([{ sequence: 201 }]), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await expect(client.listAllEvents("task/one")).resolves.toHaveLength(201);
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/tasks/task%2Fone/events?limit=200&after_sequence=0",
    );
    expect(fetchMock.mock.calls[1]?.[0]).toBe(
      "https://platform.test/api/tasks/task%2Fone/events?limit=200&after_sequence=200",
    );
  });

  it("rejects cached HTML instead of treating it as an API payload", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(new Response("<!doctype html>", {
      status: 200,
      headers: { "content-type": "text/html" },
    }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test" });

    await expect(client.health()).rejects.toThrow(
      "Server returned a non-JSON response for /api/health (HTTP 200",
    );
    expect(fetchMock.mock.calls[0]?.[1]?.cache).toBe("no-store");
  });

  it("projects the typed Provider catalog failure without exposing extra fields", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({
      kind: "provider_catalog",
      message: "secret upstream response",
      provider_catalog_failure: "timeout",
      request_id: "secret-request-id",
      retry_after_ms: 1_000,
    }), { status: 504 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test" });

    const error = await client.refreshProviderModels("provider/one").catch((value) => value);
    expect(isPlatformRequestError(error)).toBe(true);
    expect(error).toMatchObject({
      name: "PlatformRequestError",
      message: "secret upstream response",
      code: "secret upstream response",
      status: 504,
      kind: "provider_catalog",
      detail: { providerCatalogFailure: "timeout" },
    });
    expect(error).not.toHaveProperty("requestId");
    expect(error).not.toHaveProperty("detail.requestId");
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/providers/provider%2Fone/models/refresh",
    );
  });

  it("discards an unknown Provider catalog failure cause", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(new Response(JSON.stringify({
      message: "secret provider body",
      provider_catalog_failure: "secret_internal_cause",
    }), { status: 502 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test" });

    const error = await client.refreshProviderModels("provider-1").catch((value) => value);
    expect(isPlatformRequestError(error)).toBe(true);
    expect(error).toMatchObject({ detail: { providerCatalogFailure: null } });
  });

  it("uses username rather than email as the login identifier", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ session_token: "session-token" }), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test" });

    await client.login("test", "test");

    expect(fetchMock.mock.calls[0]?.[0]).toBe("https://platform.test/api/sessions");
    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      username: "test",
      password: "test",
    });
  });

  it("creates an implicit local session without login credentials", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify({ session_token: "local-session" }), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({ baseUrl: "https://platform.test" });

    await expect(client.createLocalSession()).resolves.toMatchObject({
      session_token: "local-session",
    });
    expect(fetchMock).toHaveBeenCalledWith(
      "https://platform.test/api/sessions/local",
      expect.objectContaining({ method: "POST", cache: "no-store" }),
    );
    expect(fetchMock.mock.calls[0]?.[1]?.body).toBeUndefined();
  });

  it("reads, replaces, and reuses the unified maps configuration", async () => {
    const configuration = {
      configured: true,
      provider: "mapbox",
      mapboxAccessToken: "pk.public-token",
      canConfigure: true,
      updatedAt: "2026-07-23T00:00:00Z",
    };
    const fetchMock = vi.fn()
      .mockImplementation(() => Promise.resolve(
        new Response(JSON.stringify(configuration), { status: 200 }),
      ));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.getMapsConfiguration()).resolves.toEqual(configuration);
    await expect(
      client.updateMapsConfiguration("mapbox", "pk.public-token"),
    ).resolves.toEqual(configuration);
    await expect(
      client.useMapsConfiguration(
        "018f-id",
      ),
    ).resolves.toEqual(configuration);

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/configuration/maps",
    );
    expect(fetchMock.mock.calls[1]?.[0]).toBe(
      "https://platform.test/api/configuration/maps",
    );
    expect(fetchMock.mock.calls[1]?.[1]?.method).toBe("PUT");
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toEqual({
      provider: "mapbox",
      apiKey: "pk.public-token",
    });
    expect(fetchMock.mock.calls[2]?.[0]).toBe(
      "https://platform.test/api/configuration/maps/use",
    );
    expect(JSON.parse(String(fetchMock.mock.calls[2]?.[1]?.body))).toEqual({
      approvalId: "018f-id",
    });
  });

  it("never returns an active Google Maps key from the unified resource", async () => {
    const configuration = {
      configured: true,
      provider: "google",
      mapboxAccessToken: null,
      canConfigure: true,
      updatedAt: "2026-07-23T00:00:00Z",
    };
    const fetchMock = vi.fn()
      .mockImplementation(() => Promise.resolve(
        new Response(JSON.stringify(configuration), { status: 200 }),
      ));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.getMapsConfiguration()).resolves.toEqual(configuration);
    await client.updateMapsConfiguration(
      "google",
      "google-secret",
      "018f-id",
    );
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toEqual({
      provider: "google",
      apiKey: "google-secret",
      approvalId: "018f-id",
    });
    expect(configuration).not.toHaveProperty("apiKey");
  });
});
