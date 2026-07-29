import { afterEach, describe, expect, it, vi } from "vitest";

import { PlatformClient } from "./client";

describe("PlatformClient", () => {
  afterEach(() => vi.unstubAllGlobals());

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

    await client.startRun("task/one", "workspace-one");
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
      workspace_id: "workspace-one",
      fork_thread_id: null,
      fork_source_run_id: null,
      supervisor_policy: null,
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
      .mockResolvedValueOnce(new Response(JSON.stringify(artifact), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });
    const artifactId = "8e98ff2f-82ee-4cc9-a3e6-2974debf8666";

    await expect(
      client.listRunAgentThreadTurns("run/one", "agent/one"),
    ).resolves.toEqual(turns);
    await expect(client.readArtifactContent(artifactId)).resolves.toEqual(artifact);

    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/runs/run%2Fone/agents/agent%2Fone/turns",
    );
    expect(fetchMock.mock.calls[1]?.[0]).toBe(
      `https://platform.test/api/artifacts/${artifactId}/content`,
    );
  });

  it("sends only a published Supervisor Policy reference when starting an enterprise Run", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ run: { id: "run-1" } }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("crypto", { randomUUID: () => "018f-idempotency-key" });
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await client.startRun("task-one", "workspace-one", {
      supervisorPolicy: {
        policy_id: "enterprise-supervisor-copilot",
        version: "1.0.0",
      },
    });

    expect(JSON.parse(String(fetchMock.mock.calls[0]?.[1]?.body))).toEqual({
      idempotency_key: "018f-idempotency-key",
      workspace_id: "workspace-one",
      fork_thread_id: null,
      fork_source_run_id: null,
      supervisor_policy: {
        policy_id: "enterprise-supervisor-copilot",
        version: "1.0.0",
      },
    });
  });

  it("loads one exact published Supervisor through the typed read-only route", async () => {
    const detail = {
      policy_id: "enterprise/supervisor",
      version: "1.7.0+stable",
      display_name: "Enterprise Supervisor",
      description: "Coordinates governed Agents.",
      source: "repository",
      responsibilities: ["Delegate bounded assignments."],
      instruction_policy: {
        release_id: null,
        policy_id: "platform-supervisor-behavior",
        version: "1.0.0",
        display_name: "Platform Supervisor behavior",
        description: "Platform boundaries.",
        source: "repository",
        content_sha256: "f".repeat(64),
      },
      platform_instructions: "Use only authorized Runtime capabilities.",
      custom_instructions: "Coordinate the selected Agents.",
      agents: [],
      artifact_contracts: [],
      max_active_child_agents: 2,
      content_sha256: "a".repeat(64),
      execution_semantics_sha256: "c".repeat(64),
    };
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify(detail), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(
      client.getSupervisorPolicy("enterprise/supervisor", "1.7.0+stable"),
    ).resolves.toEqual(detail);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://platform.test/api/supervisor-policies/enterprise%2Fsupervisor/1.7.0%2Bstable",
      expect.objectContaining({ cache: "no-store" }),
    );
  });

  it("publishes a platform-owned Supervisor instruction contract through its typed resource", async () => {
    const request = {
      policy_id: "platform-supervisor-behavior",
      version: "1.1.0",
      display_name: "Platform Supervisor behavior",
      description: "Updated platform boundaries.",
      platform_instructions: "Use only authorized Runtime capabilities.",
    };
    const detail = {
      ...request,
      release_id: "policy-release-1",
      source: "platform_release",
      content_sha256: "f".repeat(64),
    };
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify(detail), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.publishSupervisorInstructionPolicy(request)).resolves.toEqual(detail);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://platform.test/api/supervisor-instruction-policies",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify(request),
      }),
    );
  });

  it("loads one exact published Agent through the typed read-only route", async () => {
    const detail = {
      source: "repository",
      release_id: null,
      definition_id: "enterprise/data-agent",
      version: "1.6.0+stable",
      display_name: "Enterprise Data Agent",
      description: "Builds bounded planning data.",
      responsibilities: ["Build data."],
      developer_instructions: "Validate every published dataset.",
      input_artifact_types: [],
      output_artifact_types: ["planning-dataset.v1"],
      required_capabilities: ["supply_chain_data.build_planning_dataset"],
      capability_template: null,
      content_sha256: "b".repeat(64),
      execution_semantics_sha256: "d".repeat(64),
    };
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify(detail), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(
      client.getAgentDefinition("enterprise/data-agent", "1.6.0+stable"),
    ).resolves.toEqual(detail);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://platform.test/api/agent-definitions/enterprise%2Fdata-agent/1.6.0%2Bstable",
      expect.objectContaining({ cache: "no-store" }),
    );
  });

  it("loads the reviewed capability package directory through a typed resource", async () => {
    const packages = [{
      package_id: "map-utils",
      version: "0.1.0",
      display_name: "Map Utils",
      description: "Geocode, route, and create map cards.",
      capability_root_id: "local-maps-mcp",
      capabilities: ["Maps", "Map cards"],
      mcp_server_names: ["map_utils"],
      includes_skills: true,
      source: "repository",
    }];
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response(JSON.stringify(packages), { status: 200 }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.listCapabilityPackages()).resolves.toEqual(packages);
    expect(fetchMock).toHaveBeenCalledWith(
      "https://platform.test/api/capability-packages",
      expect.objectContaining({ cache: "no-store" }),
    );
  });

  it("validates, tests, and publishes a Python capability through Workspace resources", async () => {
    const capability = {
      slug: "stock-history",
      version: "1.0.0",
      display_name: "Stock history",
      description: "Fetch stock history.",
      server_name: "stock_data",
      python_source: "def lookup_stock(arguments): return arguments",
      tools: [{
        name: "lookup_stock",
        description: "Look up a stock.",
        input_schema: { type: "object" },
      }],
      skill: {
        name: "stock-history",
        description: "Use for stock history.",
        instructions: "Call stock_data.lookup_stock.",
      },
    };
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({
        valid: true,
        tool_names: ["lookup_stock"],
        issues: [],
      }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        tool_name: "lookup_stock",
        result: { ticker: "DEMO" },
      }), { status: 200 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        package_id: "stock-history",
        version: "1.0.0",
        capability_root_id: "local-stock-history",
        server_name: "stock_data",
        skill_name: "stock-history",
        written_files: ["tools/stock-history/.mcp.json"],
      }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await client.validatePythonCapability("workspace/one", capability);
    await client.testPythonCapability("workspace/one", {
      capability,
      tool_name: "lookup_stock",
      arguments: { name: "Example" },
    });
    await client.publishPythonCapability("workspace/one", capability);

    expect(fetchMock.mock.calls.map((call) => call[0])).toEqual([
      "https://platform.test/api/workspaces/workspace/one/python-capabilities/validate",
      "https://platform.test/api/workspaces/workspace/one/python-capabilities/test",
      "https://platform.test/api/workspaces/workspace/one/python-capabilities/publish",
    ]);
    expect(fetchMock.mock.calls.every((call) => call[1]?.method === "POST")).toBe(true);
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

  it("rejects an obsolete Run-scoped Artifact path", async () => {
    const client = new PlatformClient({ baseUrl: "https://platform.test" });

    await expect(
      client.readReplyArtifact(
        "/api/runs/8e98ff2f-82ee-4cc9-a3e6-2974debf8666/artifacts/8e98ff2f-82ee-4cc9-a3e6-2974debf8666",
      ),
    ).rejects.toThrow("Reply Artifact path is invalid.");
  });

  it("treats an absent Supervisor binding as a standard Run", async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(
      new Response("null", {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    const client = new PlatformClient({
      baseUrl: "https://platform.test",
      token: "session-token",
    });

    await expect(client.getRunSupervisorPolicy("run-1")).resolves.toBeNull();
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      "https://platform.test/api/runs/run-1/supervisor-policy",
    );
  });

  it("falls back when crypto.randomUUID is unavailable", async () => {
    const fetchMock = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ run: { id: "run-1" } }), { status: 200 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.stubGlobal("crypto", {});
    const client = new PlatformClient({ baseUrl: "https://platform.test", token: "session-token" });

    await client.startRun("task-one", "workspace-one");

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
        "http://127.0.0.1:43123/one-time-token",
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
      elicitationUrl: "http://127.0.0.1:43123/one-time-token",
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
      "http://127.0.0.1:43123/one-time-token",
    );
    expect(JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))).toEqual({
      provider: "google",
      apiKey: "google-secret",
      elicitationUrl: "http://127.0.0.1:43123/one-time-token",
    });
    expect(configuration).not.toHaveProperty("apiKey");
  });
});
