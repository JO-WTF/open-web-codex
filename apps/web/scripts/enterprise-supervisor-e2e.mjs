#!/usr/bin/env node

import assert from "node:assert/strict";
import { Blob } from "node:buffer";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { requireCompletedTurn } from "./e2e-turn-contract.mjs";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const fixtureRoot = path.join(
  repoRoot,
  "tools/supply-chain-network-planner/examples/indonesia-network/base",
);
const baseUrl = (process.env.E2E_BASE_URL ?? "http://127.0.0.1:4800").replace(
  /\/$/,
  "",
);
const apiBase = `${baseUrl}/api`;
const providerId = process.env.E2E_PROVIDER_ID ?? "deepseek-enterprise-e2e";
const providerBaseUrl =
  process.env.E2E_PROVIDER_BASE_URL ?? "https://api.deepseek.com";
const providerWireApi = process.env.E2E_PROVIDER_WIRE_API ?? "chat";
const model = process.env.E2E_MODEL ?? "deepseek-v4-flash";
const effort = process.env.E2E_EFFORT ?? "none";
const useBuiltInProvider = process.env.E2E_USE_BUILT_IN_PROVIDER === "1";
const runExtendedCases = process.env.E2E_RUN_EXTENDED_CASES !== "0";
const username = process.env.E2E_ADMIN_USERNAME ?? "enterprise-e2e";
const email =
  process.env.E2E_ADMIN_EMAIL ?? "enterprise-e2e@open-web-codex.local";
const password =
  process.env.E2E_ADMIN_PASSWORD ?? "open-web-codex-enterprise-e2e";
const repositoryPolicy = {
  policy_id: "enterprise-supervisor-copilot",
  version: "6.0.0",
};
const repositoryAgents = {
  data: { definition_id: "enterprise-data-agent", version: "6.0.0" },
  network: {
    definition_id: "enterprise-network-planning-agent",
    version: "6.0.0",
  },
};
const fixtureFiles = [
  ["demand-cities.csv", "demand_cities", "text/csv"],
  ["existing-warehouses.csv", "existing_warehouses", "text/csv"],
  ["route-quotes.csv", "route_quotes", "text/csv"],
  ["administrative-areas.json", "administrative_areas", "application/json"],
  ["candidate-warehouses.csv", "candidate_warehouses", "text/csv"],
  ["source-lock.json", "source_lock", "application/json"],
  ["validation-report.json", "validation_report", "application/json"],
];
const providerKey = useBuiltInProvider
  ? null
  : await loadSecret(
      "DEEPSEEK_API_KEY",
      process.env.DEEPSEEK_API_KEY_FILE,
    );
const secrets = [providerKey, password].filter(Boolean);
const evidenceFile = process.env.E2E_EVIDENCE_FILE
  ? path.resolve(process.env.E2E_EVIDENCE_FILE)
  : null;
const stamp = new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
const customPolicyId = `indonesia-supervisor-${stamp}`;
const state = {
  token: undefined,
  project: undefined,
  workspace: undefined,
  workState: undefined,
  dataset: undefined,
  task: undefined,
  run: undefined,
  policy: undefined,
  rootTurnIds: [],
};
const results = [];
const answeredInputs = new Set();
const approvedMcpRequests = new Set();
const approvedMcpServers = new Set([
  "platform_coordination",
  "platform_work_state",
  "supply_chain_data",
  "supply_chain_network",
]);

function sanitize(value) {
  let text = typeof value === "string" ? value : JSON.stringify(value);
  for (const secret of secrets) {
    if (secret) text = text.split(secret).join("[redacted]");
  }
  return text;
}

function log(message) {
  process.stdout.write(`${sanitize(message)}\n`);
}

async function loadSecret(envName, fileName) {
  if (fileName) return (await readFile(fileName, "utf8")).trim();
  if (process.env[envName]) return process.env[envName].trim();
  throw new Error(`${envName} or ${envName}_FILE is required`);
}

async function api(pathname, options = {}) {
  const headers = new Headers(options.headers);
  const isFormData = options.body instanceof FormData;
  if (state.token) headers.set("authorization", `Bearer ${state.token}`);
  if (options.body !== undefined && !isFormData) {
    headers.set("content-type", "application/json");
  }
  const response = await fetch(`${apiBase}${pathname}`, {
    ...options,
    headers,
    body:
      options.body === undefined
        ? undefined
        : isFormData
          ? options.body
          : JSON.stringify(options.body),
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
  if (!response.ok) {
    const error = new Error(
      `${options.method ?? "GET"} ${pathname} failed (${response.status}): ${sanitize(body)}`,
    );
    error.status = response.status;
    error.body = body;
    throw error;
  }
  return body;
}

async function eventually(probe, description, timeoutMs = 120_000, intervalMs = 500) {
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
    `${description} timed out${lastError ? `: ${sanitize(lastError.message)}` : ""}`,
  );
}

async function runCase(name, test) {
  const started = performance.now();
  log(`\n[RUN] ${name}`);
  try {
    const details = await test();
    const durationMs = Math.round(performance.now() - started);
    results.push({ name, status: "passed", durationMs, details });
    log(`[PASS] ${name} (${durationMs} ms)${details ? ` - ${details}` : ""}`);
  } catch (error) {
    const durationMs = Math.round(performance.now() - started);
    results.push({
      name,
      status: "failed",
      durationMs,
      error: sanitize(error.message),
    });
    log(`[FAIL] ${name} (${durationMs} ms) - ${error.message}`);
    throw error;
  }
}

async function allTaskEvents() {
  const events = [];
  let afterSequence = 0;
  for (;;) {
    const query = new URLSearchParams({
      after_sequence: String(afterSequence),
      limit: "200",
    });
    const page = await api(`/tasks/${state.task.id}/events?${query}`);
    events.push(...page);
    if (page.length < 200) return events;
    afterSequence = page.at(-1).sequence;
  }
}

function itemType(event) {
  return event.payload?.itemType ?? event.payload?.data?.type;
}

function completedMcpCalls(events) {
  return events.filter(
    (event) =>
      event.event_type === "codex.item.completed" &&
      itemType(event) === "mcpToolCall",
  );
}

async function approveMcpRequests() {
  const pending = await api("/approvals");
  for (const approval of pending) {
    if (
      approval.runId !== state.run.id ||
      approval.requestType !== "mcpServer/elicitation/request" ||
      approvedMcpRequests.has(approval.id)
    ) {
      continue;
    }
    const events = await allTaskEvents();
    const requestEvent = events.find(
      (event) =>
        event.event_type === "platform.approval.requested" &&
        event.payload?.data?.approvalId === approval.id,
    );
    assert(requestEvent, `Approval ${approval.id} has no durable request event`);
    const serverName = requestEvent.payload?.data?.requestParams?.serverName;
    assert(
      approvedMcpServers.has(serverName),
      `Unexpected MCP approval server: ${serverName ?? "missing"}`,
    );
    await api(`/approvals/${approval.id}/decision`, {
      method: "POST",
      body: { decision: "accept", version: approval.version },
    });
    approvedMcpRequests.add(approval.id);
  }
}

function answerFor(question) {
  if (question.options?.length) return question.options[0].label;
  const text = `${question.id} ${question.header} ${question.question}`.toLowerCase();
  if (text.includes("系数") || text.includes("coefficient")) return "1.25";
  if (text.includes("速度") || text.includes("speed")) return "40";
  if (text.includes("小时") || text.includes("hour")) return "6,12,18";
  return "使用球面距离 × 绕路系数，不调用导航接口。";
}

async function answerPendingInputs() {
  const requests = await api(`/runs/${state.run.id}/user-input-requests`);
  for (const request of requests) {
    if (answeredInputs.has(request.id)) continue;
    const answers = Object.fromEntries(
      request.questions.map((question) => [
        question.id,
        { answers: [answerFor(question)] },
      ]),
    );
    try {
      await api(`/approvals/${request.id}/user-input`, {
        method: "POST",
        body: { version: request.version, answers },
      });
      answeredInputs.add(request.id);
    } catch (error) {
      if (error.status !== 409) throw error;
      answeredInputs.add(request.id);
    }
  }
}

async function waitForTurn(turnId) {
  return eventually(
    async () => {
      await approveMcpRequests();
      await answerPendingInputs();
      const events = await allTaskEvents();
      const failure = events.find(
        (event) =>
          event.thread_id === state.run.codex_thread_id &&
          event.turn_id === turnId &&
          (event.event_type === "codex.thread.failed" ||
            event.payload?.data?.failureReason),
      );
      if (failure) throw new Error(`Root Turn failed: ${sanitize(failure.payload)}`);
      return requireCompletedTurn(events, {
        threadId: state.run.codex_thread_id,
        turnId,
        label: "Network Supervisor Turn",
        sanitize,
      });
    },
    `Network Supervisor Turn ${turnId}`,
    Number(process.env.E2E_TURN_TIMEOUT_MS ?? 900_000),
    1_000,
  );
}

async function sendMessage(text) {
  const sent = await api(`/tasks/${state.task.id}/messages`, {
    method: "POST",
    body: {
      text,
      model,
      model_provider: providerId,
      effort,
      service_tier: null,
      access_mode: "workspace-write",
      images: [],
      collaboration_mode: {
        mode: "default",
        settings: {
          model,
          reasoning_effort: effort,
        },
      },
    },
  });
  assert.equal(sent.thread_id, state.run.codex_thread_id);
  state.rootTurnIds.push(sent.turn_id);
  await waitForTurn(sent.turn_id);
  return sent.turn_id;
}

async function uploadDataset(datasetId, version, files) {
  const form = new FormData();
  form.append(
    "metadata",
    JSON.stringify({
      idempotency_key: `${datasetId}-${version}-${crypto.randomUUID()}`,
      dataset_id: datasetId,
      version,
      display_name: `Indonesia network ${version}`,
      description: "Explicitly requested tutorial fixture for E2E validation.",
      files: files.map(([logicalName, role, mediaType], index) => ({
        field_id: `file-${index + 1}`,
        logical_name: logicalName,
        role,
        media_type: mediaType,
      })),
    }),
  );
  for (const [index, [logicalName, , mediaType]] of files.entries()) {
    const bytes = await readFile(path.join(fixtureRoot, logicalName));
    form.append(
      `file-${index + 1}`,
      new Blob([bytes], { type: mediaType }),
      logicalName,
    );
  }
  const release = await api(
    `/workspaces/${encodeURIComponent(state.workspace.id)}/datasets`,
    { method: "POST", body: form },
  );
  assert.equal(release.state, "published");
  assert.equal(release.workspace_id, state.workspace.id);
  assert.equal(release.files.length, files.length);
  assert.match(release.content_sha256, /^[0-9a-f]{64}$/);
  return release;
}

async function uploadWorkspaceFiles(files) {
  const form = new FormData();
  for (const [logicalName, , mediaType] of files) {
    const bytes = await readFile(path.join(fixtureRoot, logicalName));
    form.append(
      "file",
      new Blob([bytes], { type: mediaType }),
      `tutorial/${logicalName}`,
    );
  }
  const uploaded = await api(
    `/workspaces/${encodeURIComponent(state.workspace.id)}/files`,
    { method: "POST", body: form },
  );
  assert.equal(uploaded.paths.length, files.length);
  return uploaded.paths;
}

async function createWorkState() {
  return api(`/tasks/${encodeURIComponent(state.task.id)}/work-states`, {
    method: "POST",
    body: {
      workspaceId: state.workspace.id,
      definition: {
        id: crypto.randomUUID(),
        definitionId: "indonesia-network-planning",
        version: "1.0.0",
        contentSha256: "b".repeat(64),
        components: [
          {
            key: "network_requirements",
            displayName: "本次网络问题的数据需求",
            required: true,
            resourceTypes: ["data_requirement_profile.v2"],
            dependsOn: [],
          },
          {
            key: "network_input",
            displayName: "网络输入",
            required: true,
            resourceTypes: ["normalized_network_input.v1"],
            dependsOn: ["network_requirements"],
          },
          {
            key: "route_matrix",
            displayName: "路线矩阵",
            required: false,
            resourceTypes: ["route_matrix.v1"],
            dependsOn: ["network_input"],
          },
          {
            key: "network_deliverables",
            displayName: "规划交付件",
            required: false,
            resourceTypes: ["network_planning_report.v1", "network_comparison_map.v1"],
            dependsOn: ["route_matrix"],
          },
        ],
      },
      idempotencyKey: `indonesia-work-state-${crypto.randomUUID()}`,
    },
  });
}

async function uploadCurrentCoverageExtension() {
  const logicalName = "current-coverage.csv";
  const bytes = await readFile(
    path.join(
      repoRoot,
      "tools/supply-chain-network-planner/examples/indonesia-network/current-coverage-extension",
      logicalName,
    ),
  );
  const form = new FormData();
  form.append(
    "metadata",
    JSON.stringify({
      idempotency_key: `current-coverage-${crypto.randomUUID()}`,
      dataset_id: "indonesia-warehouse-network-current-coverage",
      version: "1.0.0",
      display_name: "Indonesia current coverage extension",
      description: "Explicit current coverage extension for the tutorial.",
      files: [
        {
          field_id: "file-1",
          logical_name: logicalName,
          role: "current_coverage",
          media_type: "text/csv",
        },
      ],
    }),
  );
  form.append("file-1", new Blob([bytes], { type: "text/csv" }), logicalName);
  const release = await api(
    `/workspaces/${encodeURIComponent(state.workspace.id)}/datasets`,
    { method: "POST", body: form },
  );
  const workspaceForm = new FormData();
  workspaceForm.append(
    "file",
    new Blob([bytes], { type: "text/csv" }),
    `tutorial/${logicalName}`,
  );
  await api(`/workspaces/${encodeURIComponent(state.workspace.id)}/files`, {
    method: "POST",
    body: workspaceForm,
  });
  return release;
}

function identity(value) {
  return `${value.definition_id}@${value.version}`;
}

async function publishSupervisorDraft() {
  const policies = await api("/supervisor-policies");
  const summary = policies.find(
    (entry) =>
      entry.policy_id === repositoryPolicy.policy_id &&
      entry.version === repositoryPolicy.version,
  );
  assert(summary, "The current 6.0 Supervisor package is not published");
  const repository = await api(
    `/supervisor-policies/${repositoryPolicy.policy_id}/${repositoryPolicy.version}`,
  );
  assert.deepEqual(
    repository.agents.map(identity).sort(),
    Object.values(repositoryAgents).map(identity).sort(),
  );
  assert.equal(repository.agents.length, 2);
  assert(
    repository.artifact_contracts.every(
      (contract) => !contract.artifact_type.includes("planning-dataset.v2"),
    ),
    "The current Supervisor still requires the removed monolithic dataset contract",
  );

  const draft = {
    policy_id: customPolicyId,
    display_name: "Indonesia Network Tutorial Supervisor",
    description: "A user-authored Supervisor for the composable network tutorial.",
    responsibilities: repository.responsibilities,
    instruction_policy: {
      policy_id: repository.instruction_policy.policy_id,
      version: repository.instruction_policy.version,
    },
    custom_instructions: repository.custom_instructions,
    coordination_capabilities: repository.coordination_capabilities,
    agents: repository.agents,
    artifact_contracts: repository.artifact_contracts,
    data_requirement_contracts: repository.data_requirement_contracts ?? [],
    max_active_child_agents: 2,
  };
  let definition = await api("/supervisor-definitions", {
    method: "POST",
    body: draft,
  });
  assert.equal(definition.draft_metadata.revision, 1);
  for (let revision = 1; revision <= 2; revision += 1) {
    definition = await api(
      `/supervisor-definitions/${encodeURIComponent(definition.id)}/draft`,
      {
        method: "PUT",
        body: {
          draft: {
            ...draft,
            custom_instructions: `${draft.custom_instructions}\nRevision check ${revision}.`,
          },
          expected_revision: revision,
        },
      },
    );
    assert.equal(definition.draft_metadata.revision, revision + 1);
  }
  const validation = await api(
    `/supervisor-definitions/${encodeURIComponent(definition.id)}/validate`,
    { method: "POST" },
  );
  assert.equal(validation.valid, true, JSON.stringify(validation.issues));
  const release = await api(
    `/supervisor-definitions/${encodeURIComponent(definition.id)}/publish`,
    {
      method: "POST",
      body: { expected_revision: definition.draft_metadata.revision },
    },
  );
  assert.equal(release.version, "1.0.0");
  state.policy = { policy_id: release.policy_id, version: release.version };
  return { definition, release };
}

async function inspectRun() {
  const lifecycle = await eventually(async () => {
    const [agents, executions] = await Promise.all([
      api(`/runs/${state.run.id}/agents`),
      api(`/runs/${state.run.id}/agent-executions`),
    ]);
    const children = agents.filter((agent) => !agent.is_root);
    const childThreadIds = new Set(children.map((agent) => agent.thread_id));
    const childExecutions = executions.filter((execution) =>
      childThreadIds.has(execution.thread_id),
    );
    const terminal = new Set(["completed", "failed", "interrupted"]);
    const everyChildObserved = children.every((agent) =>
      childExecutions.some((execution) => execution.thread_id === agent.thread_id),
    );
    return children.length >= 2 &&
      everyChildObserved &&
      childExecutions.length >= children.length &&
      childExecutions.every((execution) => terminal.has(execution.status))
      ? { agents, executions, children, childExecutions }
      : undefined;
  }, "Agent execution projections to reach terminal state", 60_000);
  const { agents, executions, children, childExecutions } = lifecycle;
  const [activities, events, artifacts] = await Promise.all([
    api(`/runs/${state.run.id}/agent-activities`),
    allTaskEvents(),
    api(`/tasks/${state.task.id}/artifacts`),
  ]);
  assert(
    children.length >= 2,
    "The Supervisor did not create the required Data and Network Agent Threads",
  );
  assert(
    children.every((agent) => agent.parent_thread_id === state.run.codex_thread_id),
    "A child Agent is not attached to the root Thread",
  );
  assert(
    childExecutions.length >= children.length &&
      childExecutions.every((execution) =>
        ["completed", "failed", "interrupted"].includes(execution.status),
      ),
    "An Agent execution remained non-terminal after the root Turn completed",
  );
  assert(childExecutions.every((execution) => execution.display_title?.trim()));
  assert(childExecutions.every((execution) => Number.isInteger(execution.wait_cycle_count)));
  assert(
    !activities.some((activity) =>
      /Waiting for Agent updates|Wait cycle finished/i.test(activity.title ?? ""),
    ),
    "The main Agent timeline still contains one card per wait cycle",
  );
  const calls = completedMcpCalls(events);
  assert(calls.some((event) => event.payload?.data?.server === "supply_chain_data"));
  assert(calls.some((event) => event.payload?.data?.server === "supply_chain_network"));
  assert(calls.some((event) => event.payload?.data?.server === "platform_work_state"));
  assert(
    !calls.some((event) =>
      ["supply_chain_indonesia"].includes(event.payload?.data?.server),
    ),
    "The current run called a removed MCP entry point",
  );
  assert(
    !calls.some((event) =>
      String(event.payload?.data?.tool ?? "").includes("planning_dataset"),
    ),
    "The current run called a removed monolithic dataset tool",
  );
  const inputRequested = events.filter(
    (event) =>
      event.event_type === "platform.approval.requested" &&
      event.payload?.data?.requestMethod === "item/tool/requestUserInput",
  );
  const inputResolved = events.filter(
    (event) =>
      event.event_type === "platform.approval.resolved" &&
      event.payload?.data?.requestMethod === "item/tool/requestUserInput",
  );
  return {
    agents,
    children,
    executions,
    childExecutions,
    activities,
    events,
    artifacts,
    calls,
    inputRequested,
    inputResolved,
  };
}

await runCase("authenticated single-Profile bootstrap", async () => {
  const health = await api("/health");
  assert.equal(health.ok, true);
  let auth;
  try {
    auth = await api("/sessions/local", { method: "POST" });
  } catch (error) {
    if (error.status !== 503) throw error;
    auth = await api("/bootstrap", {
      method: "POST",
      body: { name: "Enterprise E2E Owner", username, email, password },
    });
  }
  state.token = auth.session_token;
  const me = await api("/me");
  assert.equal(me.id, auth.user.id);
  return `server=${health.version}`;
});

await runCase("real Provider selection", async () => {
  let catalog;
  if (useBuiltInProvider) {
    catalog = await api(`/providers/${providerId}/select`, { method: "POST" });
  } else {
    catalog = await api(`/providers/${providerId}`, {
      method: "PUT",
      body: {
        name: "Enterprise E2E Provider",
        baseUrl: providerBaseUrl,
        wireApi: providerWireApi,
        credentials: { mode: "direct", apiKey: providerKey },
        select: true,
      },
    });
    assert(!JSON.stringify(catalog).includes(providerKey));
    catalog = await api(`/providers/${providerId}/models/refresh`, { method: "POST" });
  }
  let provider = catalog.data.find((entry) => entry.id === providerId);
  assert(provider, `${providerId} Provider was not discovered`);
  if (!provider.models.some((entry) => entry.modelId === model)) {
    catalog = await api(
      `/providers/${encodeURIComponent(providerId)}/models/${encodeURIComponent(model)}`,
      {
        method: "PATCH",
        body: { contextWindow: 128000 },
      },
    );
    provider = catalog.data.find((entry) => entry.id === providerId);
  }
  assert(provider.models.some((entry) => entry.modelId === model), `${model} was not registered for the selected Provider`);
  await api(
    `/providers/${encodeURIComponent(providerId)}/models/${encodeURIComponent(model)}/select`,
    { method: "POST" },
  );
  return `provider=${providerId}; model=${model}`;
});

await runCase("current 6.0 capability contracts", async () => {
  const [policies, definitions, packages] = await Promise.all([
    api("/supervisor-policies"),
    api("/agent-definitions"),
    api("/capability-packages"),
  ]);
  assert(
    policies.some(
      (entry) =>
        entry.policy_id === repositoryPolicy.policy_id &&
        entry.version === repositoryPolicy.version,
    ),
  );
  for (const agent of Object.values(repositoryAgents)) {
    assert(
      definitions.some(
        (entry) =>
          entry.definition_id === agent.definition_id &&
          entry.version === agent.version &&
          entry.source === "repository",
      ),
      `${identity(agent)} is not published`,
    );
  }
  const supplyChainPackage = packages.find(
    (entry) => entry.mcp_server_names.includes("supply_chain_network"),
  );
  assert(supplyChainPackage, "The single-entry supply-chain MCP package was not discovered");
  assert(
    packages.some((entry) => entry.mcp_server_names.includes("platform_work_state")),
    "The Domain Agent Work State MCP package was not discovered",
  );
  assert(
    !packages.some((entry) => entry.mcp_server_names.includes("supply_chain_indonesia")),
    "The removed Indonesia MCP entry is still advertised",
  );
  return "6.0 Data Agent + Network Agent; one supply-chain MCP entry";
});

await runCase("managed Workspace and explicit tutorial fixture upload", async () => {
  state.project = await api("/projects/managed", {
    method: "POST",
    body: { name: `Indonesia Network Tutorial ${stamp}` },
  });
  state.workspace = await api("/workspaces", {
    method: "POST",
    body: {
      project_id: state.project.id,
      idempotency_key: `indonesia-workspace-${crypto.randomUUID()}`,
      kind: "main",
      name: state.project.name,
      source_ref: null,
      parent_workspace_id: null,
      copy_agents_md: false,
    },
  });
  state.dataset = await uploadDataset(
    "indonesia-warehouse-network-tutorial",
    "1.0.0",
    fixtureFiles,
  );
  const workspaceFiles = await uploadWorkspaceFiles(fixtureFiles);
  return `workspace=${state.workspace.id}; files=${state.dataset.files.length}; workspaceFiles=${workspaceFiles.length}`;
});

await runCase("Supervisor draft revisions and automatic release version", async () => {
  const published = await publishSupervisorDraft();
  const persistedDefinitions = await api("/supervisor-definitions");
  const persistedDefinition = persistedDefinitions.find(
    (definition) => definition.id === published.definition.id,
  );
  assert.ok(persistedDefinition, "published Supervisor definition was not persisted");
  assert.equal(persistedDefinition.draft, null);
  assert.equal(published.release.version, "1.0.0");
  return `${published.release.policy_id}@${published.release.version}; draft revisions=3`;
});

await runCase("policy-bound root Thread", async () => {
  state.task = await api("/tasks", {
    method: "POST",
    body: {
      project_id: state.project.id,
      title: "印尼仓网规划 6.0 E2E",
      model_provider: providerId,
      model,
    },
  });
  state.workState = await createWorkState();
  assert.equal(state.workState.taskId, state.task.id);
  assert.equal(state.workState.workspaceId, state.workspace.id);
  const readiness = await api(
    `/workspaces/${encodeURIComponent(state.workspace.id)}/run-readiness`,
    {
      method: "POST",
      body: {
        model_provider: providerId,
        model,
        purpose: "conversation",
        supervisor_policy: state.policy,
        agent: null,
      },
    },
  );
  assert.notEqual(readiness.status, "blocked", JSON.stringify(readiness.checks));
  const started = await api(`/tasks/${state.task.id}/runs`, {
    method: "POST",
    body: {
      idempotency_key: `indonesia-run-${crypto.randomUUID()}`,
      readiness_fingerprint: readiness.evaluation_fingerprint,
      workspace_id: state.workspace.id,
      purpose: "conversation",
      fork_thread_id: null,
      fork_source_run_id: null,
      supervisor_policy: state.policy,
    },
  });
  state.run = await eventually(async () => {
    const run = await api(`/runs/${started.run.id}`);
    return run.codex_thread_id && run.workspace_id ? run : undefined;
  }, "policy-bound root Thread");
  const binding = await eventually(async () => {
    const value = await api(`/runs/${state.run.id}/supervisor-policy`);
    return value?.state === "bound" ? value : undefined;
  });
  assert.equal(binding.policy_id, state.policy.policy_id);
  assert.equal(binding.version, state.policy.version);
  return `run=${state.run.id}`;
});

await runCase("baseline journey with explicit user input and one Work State", async () => {
  const turnId = await sendMessage(`
使用已上传的印尼仓网教程 mock 数据，明确这是用户主动要求使用的示例数据。识别国家为印度尼西亚，并让 Data Agent 与 Network Agent 在整个任务中使用同一个平台 Work State（ID: ${state.workState.id}）。
先让 Network Agent 定义本次最小数据需求，再让 Data Agent 检查 Workspace 文件、确认字段映射并把标准化结果作为 Resource 引用提交到 Work State。Agent 之间不要传 Resource URI、hash、路径、完整工具结果或原始数据。
本轮标准化输入必须与网络模型合同一致：已有仓库至少包含 warehouse_id、warehouse_name、warehouse_type、city_id、city_name、longitude、latitude、is_existing；需求城市至少包含 city_id、city_name、province_id、province_name、longitude、latitude、demand_quantity。
每个子 Agent assignment 都必须包含当前 Run ID（${state.run.id}）和 Work State ID；使用 platform_work_state 的 begin/apply/fail 工具记录操作和交付件。
路线只使用球面距离乘绕路系数，不调用导航。绕路系数尚未给出，Root Supervisor 必须在派发依赖该参数的子 Agent 前使用官方 request_user_input 询问；E2E 会回答 1.25。平均速度使用 40 km/h，目标时效为 6、12、18 小时。
计算现有仓范围内的时效最优覆盖。由于没有 current coverage，必须标记 optimized_existing_footprint 并说明不是当前实际方案。最后发布 network_planning_report.v1；本轮不计算成本、不新增仓库、不生成方案地图。`);
  const snapshot = await inspectRun();
  assert(snapshot.inputRequested.length > 0, "The missing route parameter did not create a durable input request");
  assert(
    snapshot.inputResolved.length >= snapshot.inputRequested.length,
    "The user input request was not durably resolved",
  );
  assert(
    snapshot.artifacts.every((artifact) => artifact.state === "ready"),
    "A report Artifact is not ready after the completed baseline Turn",
  );
  state.baseline = snapshot;
  return `${turnId}; children=${snapshot.children.length}; input requests=${snapshot.inputRequested.length}; calls=${snapshot.calls.length}`;
});

if (runExtendedCases) {
  await runCase("current-coverage extension and actual-current label", async () => {
    const extension = await uploadCurrentCoverageExtension();
    assert.equal(extension.state, "published");
    await sendMessage(
      "现在已经明确上传 current coverage extension。请重新检查当前覆盖关系，只计算 actual_current 的当前时效基线；仍然只使用球面距离，不要把优化基线冒充实际方案。",
    );
    const snapshot = await inspectRun();
    assert(
      snapshot.calls.some(
        (event) => event.payload?.data?.tool === "evaluate_network_baseline",
      ),
      "The current-coverage Turn did not call the composable baseline tool",
    );
    state.currentCoverage = snapshot;
    return `extension=${extension.id}; actual-current baseline requested`;
  });

  await runCase("cost, scenario, facility location and map comparison journey", async () => {
    await sendMessage(`
继续使用当前已上传的印尼教程数据。现在补充回答：使用 route quotes 计算 linehaul 和 last-mile 成本；一辆车装 1 个需求单位。
先计算成本优先覆盖和全网成本，再模拟新增一个候选仓，最后在已有仓固定的前提下执行 p-median 和 6/12/18 小时服务约束选址。求解器若返回 timeout 或非最优，必须原样标注。
所有操作继续使用已有 Work State（ID: ${state.workState.id}）和当前 Run ID。比较基线和入选方案，调用 render_network_comparison_map，输出 network_comparison_map.v1 和 network_planning_report.v1。
不要复制任何原始数据、矩阵行、完整工具结果或内部 Work State 组件标识；不要调用旧的 MCP 入口或 planning-dataset.v2 工具。`);
    const snapshot = await inspectRun();
    const tools = new Set(snapshot.calls.map((event) => event.payload?.data?.tool));
    for (const required of [
      "plan_cost_matrix",
      "evaluate_facility_scenario",
      "solve_p_median",
      "render_network_comparison_map",
      "publish_network_planning_report",
    ]) {
      assert(tools.has(required), `The extended Turn did not call ${required}`);
    }
    state.extended = snapshot;
    return `calls=${snapshot.calls.length}; artifacts=${snapshot.artifacts.length}`;
  });
}

if (evidenceFile) {
  const snapshots = [state.baseline, state.currentCoverage, state.extended].filter(Boolean);
  const evidence = {
    schemaVersion: "enterprise-supervisor-e2e.v2",
    verifiedAt: new Date().toISOString(),
    baseUrl,
    provider: { id: providerId, model },
    policy: state.policy,
    run: {
      id: state.run.id,
      rootThreadId: state.run.codex_thread_id,
      turnIds: state.rootTurnIds,
      workspaceId: state.workspace.id,
    },
    turns: snapshots.map((snapshot) => ({
      agents: snapshot.agents,
      executions: snapshot.executions,
      activities: snapshot.activities,
      inputRequestCount: snapshot.inputRequested.length,
      inputResolvedCount: snapshot.inputResolved.length,
      toolCalls: snapshot.calls.map((event) => ({
        sequence: event.sequence,
        threadId: event.thread_id,
        server: event.payload?.data?.server,
        tool: event.payload?.data?.tool,
      })),
      artifacts: snapshot.artifacts.map((artifact) => ({
        id: artifact.id,
        schema: artifact.artifact_schema,
        state: artifact.state,
        content_sha256: artifact.content_sha256,
      })),
    })),
  };
  await mkdir(path.dirname(evidenceFile), { recursive: true });
  await writeFile(evidenceFile, `${JSON.stringify(evidence, null, 2)}\n`, {
    mode: 0o600,
  });
  log(`\nEvidence: ${evidenceFile}`);
}

log("\nIndonesia Supervisor E2E summary");
for (const result of results) {
  log(`- ${result.status.toUpperCase()} ${result.name} (${result.durationMs} ms)`);
}
log(`\n${results.length}/${results.length} cases passed.`);
