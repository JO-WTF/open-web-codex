#!/usr/bin/env node

// Opt-in real Provider gate for warehouse-network-copilot.
//
// This script deliberately does not emulate a model, Agent, Skill, MCP server,
// Workspace, algorithm, or map.  It only adds a local forwarding proxy so the
// real DeepSeek request/response can be observed without logging credentials or
// full prompts/tool schemas.  The temporary Provider is removed in finally.

import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const fixtureDir = path.join(scriptDir, "fixtures", "warehouse-network", "mock_data");
const fixtureManifestPath = path.join(
  scriptDir,
  "fixtures",
  "warehouse-network",
  "mock_data",
  "manifest.json",
);
const baseUrl = (process.env.E2E_BASE_URL ?? "http://127.0.0.1:4810").replace(/\/$/, "");
const apiBase = baseUrl + "/api";
const copilotPackageId =
  process.env.E2E_COPILOT_PACKAGE_ID ?? "warehouse-network-copilot";
const providerSourceId = process.env.E2E_REAL_DEEPSEEK_SOURCE_PROVIDER_ID ?? "deepseek-e2e";
const model = process.env.E2E_REAL_DEEPSEEK_MODEL ?? "deepseek-v4-flash";
const useExistingProvider =
  (process.env.E2E_REAL_DEEPSEEK_USE_EXISTING ?? "0") === "1";
const targetBaseUrl = (
  process.env.E2E_REAL_DEEPSEEK_TARGET_BASE_URL ?? "https://api.deepseek.com"
).replace(/\/$/, "");
const username = process.env.E2E_ADMIN_USERNAME ?? "real-deepseek-e2e";
const email =
  process.env.E2E_ADMIN_EMAIL ?? "real-deepseek-e2e@open-web-codex.local";
const password =
  process.env.E2E_ADMIN_PASSWORD ?? "open-web-codex-real-deepseek-e2e";
const secrets = [password].filter(Boolean);
const stamp = new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
const results = [];
const state = { token: undefined, manifest: undefined, proxy: undefined };

class ApiError extends Error {
  constructor(method, pathname, status, body) {
    super(method + " " + pathname + " failed (" + status + "): " + sanitize(body));
    this.status = status;
    this.body = body;
  }
}

class NativeRuntimeBlocker extends Error {
  constructor(code, details) {
    super(code + ": " + JSON.stringify(details));
    this.name = "NativeRuntimeBlocker";
    this.code = code;
    this.details = details;
  }
}

function sanitize(value) {
  let text = typeof value === "string" ? value : JSON.stringify(value);
  for (const secret of [...secrets, state.token].filter(Boolean)) {
    text = text.split(secret).join("[redacted]");
  }
  return text;
}

function log(value) {
  process.stdout.write(sanitize(value) + "\n");
}

async function api(pathname, options = {}) {
  const headers = new Headers(options.headers);
  if (state.token) headers.set("authorization", "Bearer " + state.token);
  if (options.body !== undefined) headers.set("content-type", "application/json");
  const response = await fetch(apiBase + pathname, {
    ...options,
    headers,
    body: options.body === undefined ? undefined : JSON.stringify(options.body),
  });
  const text = await response.text();
  let body;
  if (text) {
    try {
      body = JSON.parse(text);
    } catch {
      body = text;
    }
  }
  if (!response.ok) throw new ApiError(options.method ?? "GET", pathname, response.status, body);
  return body;
}

async function eventually(probe, description, timeoutMs = 60_000, intervalMs = 250) {
  const deadline = Date.now() + timeoutMs;
  let lastError;
  while (Date.now() < deadline) {
    try {
      const value = await probe();
      if (value) return value;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
  throw new Error(
    description + " timed out" + (lastError ? ": " + sanitize(lastError.message) : ""),
  );
}

async function readFixtureManifest() {
  const manifest = JSON.parse(await readFile(fixtureManifestPath, "utf8"));
  assert.equal(manifest.schema_version, 1);
  assert.deepEqual(manifest.excluded_files, [".DS_Store"]);
  assert.equal(manifest.files.length, 6);
  for (const entry of manifest.files) {
    const bytes = await readFile(path.join(fixtureDir, entry.path));
    assert.equal(bytes.length, entry.bytes, entry.path + " byte count drifted");
    assert.equal(
      createHash("sha256").update(bytes).digest("hex"),
      entry.sha256,
      entry.path + " SHA-256 drifted",
    );
  }
  return manifest;
}

async function uploadWorkspaceFile(workspaceId, fixturePath, relativePath) {
  const form = new FormData();
  form.append("file", new Blob([await readFile(fixturePath)]), relativePath);
  const headers = new Headers();
  if (state.token) headers.set("authorization", "Bearer " + state.token);
  const response = await fetch(
    apiBase + "/workspaces/" + encodeURIComponent(workspaceId) + "/files",
    { method: "POST", headers, body: form },
  );
  const text = await response.text();
  if (!response.ok) {
    throw new ApiError("POST", "/workspaces/" + workspaceId + "/files", response.status, text);
  }
  return text ? JSON.parse(text) : undefined;
}

function providerId(catalog, id) {
  return (catalog.data ?? []).find((entry) => entry.id === id);
}

function summarizeToolChoice(value) {
  if (value === undefined) return "omitted";
  if (value === null) return "null";
  if (typeof value === "string") return value;
  if (typeof value === "object" && typeof value.type === "string") return "object:" + value.type;
  return "object";
}

function visibleToolNames(body) {
  if (!Array.isArray(body?.tools)) return [];
  return body.tools
    .map((tool) => tool?.function?.name ?? tool?.name ?? tool?.namespace ?? tool?.type)
    .filter((name) => typeof name === "string")
    .slice(0, 100);
}

function responseToolCalls(text) {
  const names = [];
  let structured = false;
  for (const line of text.split(/\r?\n/)) {
    const payload = line.startsWith("data: ") ? line.slice(6) : line;
    if (!payload || payload === "[DONE]") continue;
    try {
      const value = JSON.parse(payload);
      const calls = value?.choices?.flatMap((choice) => choice?.delta?.tool_calls ?? []) ?? [];
      for (const call of calls) {
        structured = true;
        const name = call?.function?.name;
        if (typeof name === "string" && !names.includes(name)) names.push(name);
      }
      const outputItems = value?.response?.output ?? [];
      for (const item of outputItems) {
        if (item?.type === "function_call" || item?.type === "tool_search_call") {
          structured = true;
          if (typeof item.name === "string" && !names.includes(item.name)) names.push(item.name);
        }
      }
    } catch {
      // Non-JSON SSE lines are intentionally not retained.
    }
  }
  return { structured, names };
}

function safeResponseHeaders(headers) {
  const result = {};
  for (const [key, value] of headers) {
    if (["content-encoding", "content-length", "transfer-encoding", "connection"].includes(key)) {
      continue;
    }
    result[key] = value;
  }
  return result;
}

class DeepSeekProbeProxy {
  constructor() {
    this.server = createServer((request, response) => {
      this.forward(request, response).catch((error) => {
        response.writeHead(502, { "content-type": "application/json" });
        response.end(JSON.stringify({ error: "real_provider_proxy_failed" }));
        this.errors.push(String(error?.message ?? error));
      });
    });
    this.address = undefined;
    this.rounds = [];
    this.errors = [];
  }

  async start() {
    await new Promise((resolve, reject) => {
      this.server.once("error", reject);
      this.server.listen(0, "127.0.0.1", resolve);
    });
    const address = this.server.address();
    this.address = "http://127.0.0.1:" + address.port;
    return this;
  }

  async close() {
    await new Promise((resolve) => this.server.close(() => resolve()));
  }

  async forward(request, response) {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    const bodyText = Buffer.concat(chunks).toString("utf8");
    let body;
    try {
      body = bodyText ? JSON.parse(bodyText) : {};
    } catch {
      body = {};
    }
    const isChat = request.url?.includes("/chat/completions") ?? false;
    const requestTools = visibleToolNames(body);
    const metadata = {
      round: this.rounds.length + 1,
      path: request.url ?? "unknown",
      wire_api: isChat ? "chat" : "responses",
      tools_present: Array.isArray(body.tools) && body.tools.length > 0,
      tool_count: Array.isArray(body.tools) ? body.tools.length : 0,
      tool_choice: summarizeToolChoice(body.tool_choice),
      visible_tool_names: requestTools,
    };
    const headers = new Headers();
    for (const [key, value] of Object.entries(request.headers)) {
      if (value === undefined || ["host", "content-length", "connection"].includes(key)) continue;
      headers.set(key, Array.isArray(value) ? value.join(",") : value);
    }
    const upstream = await fetch(targetBaseUrl + (request.url ?? "/"), {
      method: request.method,
      headers,
      body: bodyText || undefined,
    });
    const responseText = await upstream.text();
    const calls = responseToolCalls(responseText);
    this.rounds.push({
      ...metadata,
      status: upstream.status,
      structured_tool_calls: calls.structured,
      tool_call_count: calls.names.length,
      wire_tool_names: calls.names,
    });
    response.writeHead(upstream.status, safeResponseHeaders(upstream.headers));
    response.end(responseText);
  }
}

function itemType(event) {
  return event.payload?.itemType ?? event.payload?.data?.type;
}

function eventData(event) {
  return event.payload?.data ?? {};
}

function eventTool(event) {
  const data = eventData(event);
  return data.tool ?? data.name ?? data.data?.tool ?? data.result?.tool ?? "";
}

function nativeToolNames(events) {
  const names = [];
  for (const event of events) {
    const data = eventData(event);
    const namespace = data.namespace ?? data.toolNamespace;
    const name = data.name ?? data.toolName;
    if (typeof namespace === "string" && typeof name === "string") {
      const value = namespace + "." + name;
      if (!names.includes(value)) names.push(value);
    } else if (typeof data.tool === "string" && !names.includes(data.tool)) {
      names.push(data.tool);
    }
  }
  return names;
}

function eventSummary(events) {
  return events.map((event) => ({
    event_type: event.event_type,
    turn_id: event.turn_id,
    item_type: itemType(event),
    tool: eventTool(event) || undefined,
  }));
}

async function ensureAuthenticated() {
  const health = await api("/health");
  assert.equal(health.ok, true);
  let auth;
  try {
    auth = await api("/bootstrap", {
      method: "POST",
      body: { name: "Real DeepSeek E2E Owner", username, email, password },
    });
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 409) throw error;
    auth = await api("/sessions/local", { method: "POST" });
  }
  state.token = auth.session_token;
  return health.version;
}

function installationId(status, packageId) {
  return (status.installations ?? []).find(
    (entry) => (entry.package_id ?? entry.packageId) === packageId,
  );
}

async function ensureCopilotActive() {
  let status = await api("/profile/copilots");
  for (const installation of status.installations ?? []) {
    const packageId = installation.package_id ?? installation.packageId;
    if (installation.active && packageId !== copilotPackageId) {
      await api("/profile/copilots/deactivate", {
        method: "POST",
        body: { packageId },
      });
    }
  }
  const current = installationId(status, copilotPackageId);
  if (!current?.active || String(current.state).toLowerCase() !== "ready") {
    if (current?.active) {
      await api("/profile/copilots/deactivate", {
        method: "POST",
        body: { packageId: copilotPackageId },
      });
      await eventually(
        async () => {
          const next = await api("/profile/copilots");
          return installationId(next, copilotPackageId)?.active === false ? next : undefined;
        },
        "Copilot deactivation",
        120_000,
        1_000,
      );
    }
    await api("/profile/copilots/activate", {
      method: "POST",
      body: { packageId: copilotPackageId },
    });
  }
  status = await api("/profile/copilots");
  const target = installationId(status, copilotPackageId);
  assert(target?.active === true);
  return target;
}

async function enableMultiAgent() {
  const settings = await api("/profile/agents/settings", {
    method: "PUT",
    body: { multiAgentEnabled: true, maxThreads: 4, maxDepth: 3 },
  });
  assert.equal(settings.multiAgentEnabled ?? settings.multi_agent_enabled, true);
}

async function configureTemporaryProvider() {
  const catalog = await api("/providers");
  const source = providerId(catalog, providerSourceId);
  if (!source) {
    throw new NativeRuntimeBlocker("provider_not_configured", {
      provider_id: providerSourceId,
      available_provider_ids: (catalog.data ?? []).map((entry) => entry.id),
    });
  }
  if (source.kind === "builtIn" || source.kind === "BuiltIn") {
    throw new NativeRuntimeBlocker("provider_not_editable", { provider_id: providerSourceId });
  }
  if (!source.envKey && !source.env_key) {
    throw new NativeRuntimeBlocker("provider_credentials_unavailable", {
      provider_id: providerSourceId,
      credential_configured: false,
    });
  }
  if (useExistingProvider) {
    const originalSupports = process.env.E2E_REAL_DEEPSEEK_ORIGINAL_SUPPORTS_FUNCTION_TOOLS;
    if (!["0", "1"].includes(originalSupports ?? "")) {
      throw new NativeRuntimeBlocker("provider_capability_restore_contract_missing", {
        provider_id: providerSourceId,
        required_environment: "E2E_REAL_DEEPSEEK_ORIGINAL_SUPPORTS_FUNCTION_TOOLS=0|1",
      });
    }
    await api("/providers/" + encodeURIComponent(source.id), {
      method: "PUT",
      body: {
        name: source.name,
        baseUrl: state.proxy.address + "/v1",
        wireApi: "chat",
        supportsFunctionTools: true,
        credentials: { mode: "preserve" },
        select: false,
      },
    });
    return {
      id: source.id,
      source,
      restoreExisting: true,
      originalSupportsFunctionTools: originalSupports === "1",
      originalBaseUrl: source.baseUrl,
      originalWireApi: source.wireApi,
      originalName: source.name,
    };
  }
  const temporaryId = "deepseek-real-e2e-" + stamp.toLowerCase();
  const envKey = source.envKey ?? source.env_key;
  const result = await api("/providers/" + encodeURIComponent(temporaryId), {
    method: "PUT",
    body: {
      name: "DeepSeek Real E2E (temporary)",
      baseUrl: state.proxy.address + "/v1",
      wireApi: "chat",
      supportsFunctionTools: true,
      credentials: { mode: "environment", envKey },
      select: false,
    },
  });
  const configured = providerId(result, temporaryId);
  assert(configured);
  // The current ProviderService selects a newly-created Provider as part of
  // its atomic upsert.  Keep that selection for this Run: Runtime resolves
  // the task's Provider from the current config, and the original selection
  // is restored immediately before deleting the temporary Provider.
  const restoreProviderId = catalog.currentProviderId ?? catalog.current_provider_id;
  return {
    id: temporaryId,
    source,
    modelProviderCatalog: result,
    restoreProviderId,
    restoreModelId: catalog.currentModelId ?? catalog.current_model_id,
  };
}

async function removeTemporaryProvider(provider) {
  if (!provider?.id) return;
  if (provider.restoreExisting) {
    await api("/providers/" + encodeURIComponent(provider.id), {
      method: "PUT",
      body: {
        name: provider.originalName,
        baseUrl: provider.originalBaseUrl,
        wireApi: provider.originalWireApi,
        supportsFunctionTools: provider.originalSupportsFunctionTools,
        credentials: { mode: "preserve" },
        select: false,
      },
    }).catch(() => undefined);
    return;
  }
  if (provider.restoreProviderId) {
    await api("/providers/" + encodeURIComponent(provider.restoreProviderId) + "/select", {
      method: "POST",
    }).catch(() => undefined);
    if (provider.restoreModelId) {
      await api(
        "/providers/" +
          encodeURIComponent(provider.restoreProviderId) +
          "/models/" +
          encodeURIComponent(provider.restoreModelId) +
          "/select",
        { method: "POST" },
      ).catch(() => undefined);
    }
  }
  await api("/providers/" + encodeURIComponent(provider.id), { method: "DELETE" }).catch(
    () => undefined,
  );
}

async function createTaskAndRun(providerIdValue, title) {
  const project = await api("/projects/managed", {
    method: "POST",
    body: { name: title + " project" },
  });
  const workspace = await api("/workspaces", {
    method: "POST",
    body: {
      project_id: project.id,
      idempotency_key: "real-deepseek-workspace-" + randomUUID(),
      kind: "main",
      name: title + " workspace",
      source_ref: null,
      parent_workspace_id: null,
      copy_agents_md: false,
    },
  });
  const record = { project, workspace, task: undefined, run: undefined };
  try {
    for (const entry of state.manifest.files) {
      await uploadWorkspaceFile(
        workspace.id,
        path.join(fixtureDir, entry.path),
        "mock_data/" + entry.path,
      );
    }
    const task = await api("/tasks", {
      method: "POST",
      body: {
        project_id: project.id,
        workspace_id: workspace.id,
        title,
        model_provider: providerIdValue,
        model,
        copilot_package_id: copilotPackageId,
      },
    });
    record.task = task;
    const started = await api("/tasks/" + task.id + "/runs", {
      method: "POST",
      body: { idempotency_key: "real-deepseek-run-" + randomUUID() },
    });
    record.run = await eventually(
      async () => {
        const current = await api("/runs/" + (started.run ?? started).id);
        return current.codex_thread_id ? current : undefined;
      },
      "Real DeepSeek Run readiness",
      90_000,
      500,
    );
    return record;
  } catch (error) {
    await cleanupCase(record);
    throw error;
  }
}

async function send(taskId) {
  return api("/tasks/" + taskId + "/messages", {
    method: "POST",
    body: {
      text: [
        "根据已上传的 mock_data 文件，计算 12 小时时效达标率并展示地图。",
        "请使用当前 warehouse-network-copilot 的 native multi-agent 协同：清理 raw data，",
        "将处理后的输入留在 Workspace，由 network agent 完成路线、12h 求解和地图交付。",
      ].join("\n"),
      model,
      model_provider: taskProviderId,
      effort: "none",
      service_tier: null,
      access_mode: "workspace-write",
      images: [],
      collaboration_mode: null,
    },
  });
}

async function taskEvents(taskId) {
  return api("/tasks/" + taskId + "/events?limit=5000");
}

async function waitForTurn(taskId, turnId, timeoutMs = 300_000) {
  return eventually(
    async () => {
      const events = await taskEvents(taskId);
      const failure = events.find(
        (event) =>
          event.turn_id === turnId &&
          (event.event_type === "codex.thread.failed" ||
            event.payload?.data?.failureReason ||
            event.payload?.data?.artifactDelivery?.state === "failed"),
      );
      if (failure) throw new Error("Turn failed: " + sanitize(failure.event_type));
      const approval = events.find(
        (event) =>
          event.turn_id === turnId && event.event_type === "platform.approval.requested",
      );
      if (approval) {
        throw new NativeRuntimeBlocker("approval_required", {
          canonical_item_id: approval.item_id,
          canonical_item_type: approval.payload?.itemType,
        });
      }
      return events.some(
        (event) => event.turn_id === turnId && event.event_type === "codex.turn.completed",
      )
        ? events
        : undefined;
    },
    "Real DeepSeek Turn completion",
    timeoutMs,
    500,
  );
}

async function cleanupRun(runId) {
  if (!runId) return;
  try {
    const run = await api("/runs/" + runId);
    if (["running", "queued", "recovery_pending"].includes(run.status)) {
      await api("/runs/" + runId + "/cancel", { method: "POST" });
    }
  } catch {
    // Preserve the original gate result; cleanup is retried by the terminal probe.
  }
  await eventually(
    async () => {
      try {
        const run = await api("/runs/" + runId);
        return !["running", "queued", "recovery_pending"].includes(run.status);
      } catch {
        return true;
      }
    },
    "Run cleanup",
    30_000,
    500,
  ).catch(() => undefined);
}

async function cleanupCase(record) {
  await cleanupRun(record?.run?.id);
  if (record?.workspace?.id) {
    await api("/workspaces/" + record.workspace.id, { method: "DELETE" }).catch(
      () => undefined,
    );
  }
  if (record?.project?.id) {
    await api("/projects/" + record.project.id, { method: "DELETE" }).catch(
      () => undefined,
    );
  }
}

let taskProviderId;

async function runGate(provider) {
  const record = await createTaskAndRun(provider.id, "Real DeepSeek 12h network " + stamp);
  try {
    const response = await send(record.task.id);
    assert(response.turn_id);
    const firstRound = await eventually(
      async () => state.proxy.rounds.find((round) => round.wire_api === "chat"),
      "first real Chat request",
      180_000,
      250,
    );
    const minimalEvidence = {
      round: firstRound.round,
      tools_present: firstRound.tools_present,
      tool_count: firstRound.tool_count,
      tool_choice: firstRound.tool_choice,
      visible_tool_names: firstRound.visible_tool_names,
      structured_tool_calls: firstRound.structured_tool_calls,
      wire_tool_names: firstRound.wire_tool_names,
      model_supports_native_tool_search:
        firstRound.visible_tool_names.includes("tool_search"),
    };
    const canonicalBeforeStop = await taskEvents(record.task.id)
      .then((events) => eventSummary(events).filter((event) => event.turn_id === response.turn_id))
      .catch(() => []);
    log("[ROUND 1] " + JSON.stringify(minimalEvidence));
    if (!firstRound.structured_tool_calls) {
      throw new NativeRuntimeBlocker("provider_tool_call_not_produced", {
        provider_id: provider.id,
        model,
        evidence: minimalEvidence,
        canonical_terminal: canonicalBeforeStop,
      });
    }
    if (!firstRound.visible_tool_names.includes("tool_search")) {
      throw new NativeRuntimeBlocker("provider_tool_search_capability_unavailable", {
        provider_id: provider.id,
        model,
        evidence: minimalEvidence,
        canonical_terminal: canonicalBeforeStop,
      });
    }

    const events = await waitForTurn(record.task.id, response.turn_id);
    const nativeNames = nativeToolNames(events);
    const rounds = state.proxy.rounds.map((round) => ({
      round: round.round,
      tools_present: round.tools_present,
      tool_count: round.tool_count,
      tool_choice: round.tool_choice,
      visible_tool_names: round.visible_tool_names,
      structured_tool_calls: round.structured_tool_calls,
      wire_tool_names: round.wire_tool_names,
    }));
    log("[ROUNDS] " + JSON.stringify(rounds));
    log(
      "[CANONICAL] " +
        JSON.stringify({
          native_tool_names: nativeNames,
          terminal_events: eventSummary(events).filter((event) =>
            /completed|failed|cancelled|error/i.test(event.event_type ?? ""),
          ),
        }),
    );
    const eventText = events.map((event) => sanitize(event.payload)).join("\n");
    if (!/12\s*h|12\s*小时|12-hour/i.test(eventText)) {
      throw new NativeRuntimeBlocker("copilot_chain_incomplete", {
        reason: "12h_result_not_reported",
        provider_id: provider.id,
        rounds,
        native_tool_names: nativeNames,
      });
    }
    if (!events.some((event) => /create_network_map_card/.test(String(eventTool(event))))) {
      throw new NativeRuntimeBlocker("copilot_chain_incomplete", {
        reason: "map_producer_item_not_projected",
        provider_id: provider.id,
        rounds,
        native_tool_names: nativeNames,
      });
    }
    return {
      status: "passed",
      provider_id: provider.id,
      model,
      round_count: rounds.length,
      native_tool_names: nativeNames,
    };
  } finally {
    await cleanupCase(record);
  }
}

async function main() {
  if (process.env.E2E_REAL_DEEPSEEK !== "1") {
    log("[SKIP] real DeepSeek gate is opt-in; set E2E_REAL_DEEPSEEK=1");
    return;
  }
  state.manifest = await readFixtureManifest();
  state.proxy = await new DeepSeekProbeProxy().start();
  let provider;
  try {
    const version = await ensureAuthenticated();
    await ensureCopilotActive();
    await enableMultiAgent();
    provider = await configureTemporaryProvider();
    taskProviderId = provider.id;
    log(
      "server=" +
        version +
        " fixture_files=" +
        state.manifest.files.length +
        " provider=" +
        provider.id,
    );
    const started = Date.now();
    try {
      const details = await runGate(provider);
      results.push({ name: "real DeepSeek multi-agent gate", ...details, durationMs: Date.now() - started });
      log("[PASS] real DeepSeek multi-agent gate");
    } catch (error) {
      if (error instanceof NativeRuntimeBlocker) {
        results.push({
          name: "real DeepSeek multi-agent gate",
          status: "typed_failure",
          code: error.code,
          details: error.details,
          durationMs: Date.now() - started,
        });
        throw error;
      }
      throw error;
    }
  } finally {
    await removeTemporaryProvider(provider);
    await state.proxy.close();
  }
  log("Real DeepSeek E2E summary");
  for (const result of results) log("- " + JSON.stringify(result));
}

main().catch((error) => {
  if (error instanceof NativeRuntimeBlocker) {
    log("[TYPED_FAILURE] " + error.message);
    process.exitCode = 2;
  } else {
    log("[FAIL] " + error.stack);
    process.exitCode = 1;
  }
});
