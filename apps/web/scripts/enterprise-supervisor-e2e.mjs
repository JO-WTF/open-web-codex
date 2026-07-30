#!/usr/bin/env node

import assert from "node:assert/strict";
import { Blob } from "node:buffer";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const indonesiaReleaseRoot = path.join(
  repoRoot,
  "tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0",
);
const baseUrl = (process.env.E2E_BASE_URL ?? "http://127.0.0.1:4810").replace(/\/$/, "");
const apiBase = `${baseUrl}/api`;
const providerId = process.env.E2E_PROVIDER_ID ?? "deepseek-enterprise-e2e";
const providerBaseUrl = process.env.E2E_PROVIDER_BASE_URL ?? "https://api.deepseek.com";
const providerWireApi = process.env.E2E_PROVIDER_WIRE_API ?? "chat";
const model = process.env.E2E_MODEL ?? "deepseek-v4-flash";
const effort = process.env.E2E_EFFORT ?? "none";
const useBuiltInProvider = process.env.E2E_USE_BUILT_IN_PROVIDER === "1";
const promptOverride = process.env.E2E_PROMPT?.trim();
const observationOnly = process.env.E2E_OBSERVE_ONLY === "1";
const lifecycleProbe = process.env.E2E_LIFECYCLE_PROBE === "1";
const caseName = process.env.E2E_CASE_NAME?.trim() || "Indonesia Web authoring E2E";
const username = process.env.E2E_ADMIN_USERNAME ?? "enterprise-e2e";
const email = process.env.E2E_ADMIN_EMAIL ?? "enterprise-e2e@open-web-codex.local";
const password = process.env.E2E_ADMIN_PASSWORD ?? "open-web-codex-enterprise-e2e";
const repositoryPolicy = {
  policy_id: "enterprise-supervisor-copilot",
  version: "3.10.0",
};
const repositoryAgents = {
  data: { definition_id: "enterprise-data-agent", version: "3.1.0" },
  network: {
    definition_id: "enterprise-network-planning-agent",
    version: "3.4.0",
  },
  visualization: {
    definition_id: "enterprise-visualization-agent",
    version: "1.3.0",
  },
};
const providerKey = useBuiltInProvider
  ? null
  : await loadSecret("DEEPSEEK_API_KEY", process.env.DEEPSEEK_API_KEY_FILE);
const evidenceFile = process.env.E2E_EVIDENCE_FILE
  ? path.resolve(process.env.E2E_EVIDENCE_FILE)
  : null;
const secrets = [providerKey, password].filter(Boolean);
const stamp = new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
const customVersion = "1.0.0";
const customAgentIds = {
  data: `indonesia-data-${stamp}`,
  network: `indonesia-network-${stamp}`,
  visualization: `indonesia-map-${stamp}`,
};
const customPolicyId = `indonesia-supervisor-${stamp}`;
const indonesiaFileContract = [
  ["dataset-manifest.json", "dataset_manifest", "application/json"],
  ["province-boundaries.geojson", "province_boundaries", "application/geo+json"],
  ["customers.csv.gz", "customers", "application/gzip"],
  ["customer-assignments.csv.gz", "customer_assignments", "application/gzip"],
  ["warehouses.csv", "warehouses", "text/csv"],
  ["warehouse-links.csv", "warehouse_links", "text/csv"],
  ["candidate-locations.csv", "candidate_locations", "text/csv"],
  ["transport-quotes.csv", "transport_quotes", "text/csv"],
  ["planning-policy.json", "planning_policy", "application/json"],
  ["validation-report.json", "validation_report", "application/json"],
];
const reportSections = [
  ["事实", "Facts?"],
  ["假设", "Assumptions?"],
  ["分析", "Analysis"],
  ["建议", "Recommendations?"],
  ["局限", "Limitations?"],
  ["缺失证据", "Missing Evidence"],
];
const state = {
  token: undefined,
  project: undefined,
  workspace: undefined,
  datasetRelease: undefined,
  repositoryAgentDetails: {},
  repositorySupervisor: undefined,
  agentReleases: {},
  supervisorRelease: undefined,
  policy: undefined,
  task: undefined,
  run: undefined,
  turnId: undefined,
  capabilityPackages: {},
};
const results = [];
const approvedEnterpriseMcpRequests = new Set();
const approvedMcpServers = new Set(["supply_chain_indonesia", "map_utils"]);

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
    throw new Error(
      `${options.method ?? "GET"} ${pathname} failed (${response.status}): ${sanitize(body)}`,
    );
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
    log(`[PASS] ${name} (${durationMs} ms)${details ? ` — ${details}` : ""}`);
  } catch (error) {
    const durationMs = Math.round(performance.now() - started);
    results.push({ name, status: "failed", durationMs, error: sanitize(error.message) });
    log(`[FAIL] ${name} (${durationMs} ms) — ${error.message}`);
    throw error;
  }
}

function findProvider(catalog, id) {
  return catalog.data.find((provider) => provider.id === id);
}

function currentProviderId(catalog) {
  return catalog.currentProviderId ?? catalog.current_provider_id;
}

function identity(definition) {
  return `${definition.definition_id}@${definition.version}`;
}

async function allTaskEvents(taskId) {
  const events = [];
  let afterSequence = 0;
  for (;;) {
    const query = new URLSearchParams({ limit: "200" });
    query.set("after_sequence", String(afterSequence));
    const page = await api(`/tasks/${taskId}/events?${query}`);
    events.push(...page);
    if (page.length < 200) return events;
    afterSequence = page.at(-1).sequence;
  }
}

async function approvePendingEnterpriseMcpRequests() {
  const pending = await api("/approvals");
  for (const approval of pending) {
    if (
      approval.runId !== state.run.id ||
      approval.requestType !== "mcpServer/elicitation/request" ||
      approvedEnterpriseMcpRequests.has(approval.id)
    ) {
      continue;
    }
    const events = await allTaskEvents(state.task.id);
    const requestEvent = events.find(
      (event) =>
        event.event_type === "platform.approval.requested" &&
        event.payload?.data?.approvalId === approval.id,
    );
    assert(requestEvent, `Approval ${approval.id} omitted its durable request event`);
    const requestParams = requestEvent.payload?.data?.requestParams;
    assert(
      approvedMcpServers.has(requestParams?.serverName),
      `Unexpected MCP approval server: ${requestParams?.serverName ?? "missing"}`,
    );
    assert(
      typeof requestParams?.message === "string" &&
        requestParams.message.trim().length > 0,
      `MCP approval ${approval.id} omitted its display message`,
    );
    await api(`/approvals/${approval.id}/decision`, {
      method: "POST",
      body: {
        decision: "accept",
        version: approval.version,
      },
    });
    approvedEnterpriseMcpRequests.add(approval.id);
  }
}

async function waitForTurn(taskId, turnId) {
  return eventually(
    async () => {
      await approvePendingEnterpriseMcpRequests();
      const events = await allTaskEvents(taskId);
      const rootFailure = events.find(
        (event) =>
          event.thread_id === state.run.codex_thread_id &&
          event.turn_id === turnId &&
          (event.event_type === "codex.thread.failed" ||
            event.payload?.data?.failureReason),
      );
      if (rootFailure) {
        throw new Error(`Root Turn failed: ${sanitize(rootFailure.payload)}`);
      }
      return events.some(
        (event) =>
          event.thread_id === state.run.codex_thread_id &&
          event.turn_id === turnId &&
          event.event_type === "codex.turn.completed",
      )
        ? events
        : undefined;
    },
    `Enterprise Supervisor Turn ${turnId}`,
    Number(process.env.E2E_TURN_TIMEOUT_MS ?? 900_000),
    1_000,
  );
}

async function waitForTurnStarted(taskId, turnId) {
  return eventually(
    async () => {
      const events = await allTaskEvents(taskId);
      return events.some(
        (event) =>
          event.thread_id === state.run.codex_thread_id &&
          event.turn_id === turnId &&
          event.event_type === "codex.turn.started",
      )
        ? events
        : undefined;
    },
    `Runtime start for Turn ${turnId}`,
    120_000,
    100,
  );
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

function findToolCall(calls, server, tool) {
  return calls.find(
    (event) =>
      event.payload?.data?.server === server && event.payload?.data?.tool === tool,
  );
}

function hasSection(report, chinese, english) {
  return new RegExp(
    `(^|\\n)#{1,4}\\s*(?:[一二三四五六七八九十0-9]+[、.．):：-]\\s*)?(?:${chinese}|${english})`,
    "im",
  ).test(report);
}

function percentage(value) {
  return `${(value * 100).toFixed(2)}%`;
}

function reportHasInteger(report, value) {
  return report.replaceAll(/[\s,_]/g, "").includes(String(value));
}

function escapeRegExp(value) {
  return value.replaceAll(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function rankedProvincePattern(codes, provincesByCode) {
  return new RegExp(
    codes
      .map((code) => {
        const province = provincesByCode.get(code);
        assert(province, `Current network Artifact omitted province ${code}`);
        return `(?:${escapeRegExp(code)}|${escapeRegExp(province.province_name)})`;
      })
      .join("[\\s\\S]{0,1200}?"),
    "i",
  );
}

function lastFinalReport(events, turnId = state.turnId) {
  return events
    .filter(
      (event) =>
        event.thread_id === state.run.codex_thread_id &&
        event.turn_id === turnId &&
        event.event_type === "codex.item.completed" &&
        itemType(event) === "agentMessage",
    )
    .map((event) => event.payload?.data?.text)
    .filter((text) => typeof text === "string" && text.trim())
    .findLast((text) =>
      reportSections.every(([chinese, english]) =>
        hasSection(text, chinese, english),
      ),
    );
}

function finalReportEvent(events) {
  return events.findLast(
    (event) =>
      event.thread_id === state.run.codex_thread_id &&
      event.turn_id === state.turnId &&
      event.event_type === "codex.item.completed" &&
      itemType(event) === "agentMessage" &&
      Array.isArray(event.payload?.data?.inlineArtifacts),
  );
}

async function publishAgentRelease(kind, datasetReleaseIds) {
  const templateIdentity = repositoryAgents[kind];
  const template = state.repositoryAgentDetails[kind];
  const draft = {
    definition_id: customAgentIds[kind],
    version: customVersion,
    display_name: `${template.display_name} Web E2E`,
    description: template.description,
    responsibilities: template.responsibilities,
    developer_instructions: template.developer_instructions,
    input_artifact_types: template.input_artifact_types,
    output_artifact_types: template.output_artifact_types,
    capability_template: {
      source: "repository_agent",
      definition_id: templateIdentity.definition_id,
      version: templateIdentity.version,
      release_id: null,
    },
    dataset_release_ids: datasetReleaseIds,
  };
  const resource = await api("/agent-definition-resources", {
    method: "POST",
    body: draft,
  });
  const validation = await api(
    `/agent-definition-resources/${encodeURIComponent(resource.id)}/validate`,
    { method: "POST" },
  );
  assert.equal(validation.valid, true, JSON.stringify(validation.issues));
  assert.match(validation.content_sha256, /^[0-9a-f]{64}$/);
  assert.match(validation.execution_semantics_sha256, /^[0-9a-f]{64}$/);
  const release = await api(
    `/agent-definition-resources/${encodeURIComponent(resource.id)}/publish`,
    { method: "POST" },
  );
  assert.equal(release.definition_id, customAgentIds[kind]);
  assert.equal(release.version, customVersion);
  return { draft, resource, release };
}

function remapAgentIdentity(value) {
  if (value === "supervisor") return value;
  for (const [kind, repositoryAgent] of Object.entries(repositoryAgents)) {
    if (value === identity(repositoryAgent)) {
      return `${customAgentIds[kind]}@${customVersion}`;
    }
  }
  throw new Error(`Unknown repository Agent identity in Supervisor contract: ${value}`);
}

await runCase("authenticated single-Profile bootstrap", async () => {
  const health = await api("/health");
  assert.equal(health.ok, true);
  let auth;
  try {
    auth = await api("/sessions/local", { method: "POST" });
  } catch (error) {
    if (!error.message.includes("503")) throw error;
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
    assert(!JSON.stringify(catalog).includes(providerKey), "Provider response leaked its API key");
    catalog = await api(`/providers/${providerId}/models/refresh`, { method: "POST" });
  }
  assert.equal(currentProviderId(catalog), providerId);
  const provider = findProvider(catalog, providerId);
  assert(provider, `${providerId} Provider was not discovered`);
  if (!useBuiltInProvider || provider.models.length > 0) {
    assert(
      provider.models.some((entry) => entry.modelId === model),
      `${model} was not discovered; available=${provider.models
        .map((entry) => entry.modelId)
        .join(",")}`,
    );
  }
  return `provider=${providerId}`;
});

await runCase("current repository capability contracts", async () => {
  const [policies, definitions, capabilityPackages, supervisor] = await Promise.all([
    api("/supervisor-policies"),
    api("/agent-definitions"),
    api("/capability-packages"),
    api(`/supervisor-policies/${repositoryPolicy.policy_id}/${repositoryPolicy.version}`),
  ]);
  assert(
    policies.some(
      (entry) =>
        entry.policy_id === repositoryPolicy.policy_id &&
        entry.version === repositoryPolicy.version,
    ),
  );
  for (const [kind, agentIdentity] of Object.entries(repositoryAgents)) {
    assert(
      definitions.some(
        (entry) =>
          entry.definition_id === agentIdentity.definition_id &&
          entry.version === agentIdentity.version &&
          entry.source === "repository",
      ),
      `${identity(agentIdentity)} is not published`,
    );
    state.repositoryAgentDetails[kind] = await api(
      `/agent-definitions/${agentIdentity.definition_id}/${agentIdentity.version}`,
    );
  }
  state.repositorySupervisor = supervisor;
  assert.deepEqual(
    supervisor.agents.map((entry) => `${entry.definition_id}@${entry.version}`).sort(),
    Object.values(repositoryAgents).map(identity).sort(),
  );
  const supplyChainPackages = capabilityPackages.filter(
    (entry) =>
      entry.includes_skills &&
      entry.mcp_server_names.includes("supply_chain_indonesia"),
  );
  const mapPackages = capabilityPackages.filter((entry) =>
    entry.mcp_server_names.includes("map_utils"),
  );
  assert.equal(supplyChainPackages.length, 1, "Indonesia capability discovery was ambiguous");
  assert.equal(mapPackages.length, 1, "Map capability discovery was ambiguous");
  state.capabilityPackages.supplyChain = supplyChainPackages[0];
  state.capabilityPackages.maps = mapPackages[0];
  return `${repositoryPolicy.policy_id}@${repositoryPolicy.version}; 3 governed roles`;
});

await runCase("managed Workspace", async () => {
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
  assert.equal(state.workspace.project_id, state.project.id);
  return `workspace=${state.workspace.id}`;
});

await runCase("Web-published immutable Indonesia Dataset Release", async () => {
  const form = new FormData();
  const metadata = {
    idempotency_key: `indonesia-dataset-${crypto.randomUUID()}`,
    dataset_id: "indonesia-warehouse-network-tutorial",
    version: "1.0.0",
    display_name: "Indonesia Warehouse Network Tutorial",
    description:
      "Synthetic, source-locked tutorial data for governed Indonesia warehouse-network analysis.",
    files: indonesiaFileContract.map(([logicalName, role, mediaType], index) => ({
      field_id: `file-${index + 1}`,
      logical_name: logicalName,
      role,
      media_type: mediaType,
    })),
  };
  form.append("metadata", JSON.stringify(metadata));
  for (const [index, [logicalName, , mediaType]] of indonesiaFileContract.entries()) {
    const bytes = await readFile(path.join(indonesiaReleaseRoot, logicalName));
    form.append(
      `file-${index + 1}`,
      new Blob([bytes], { type: mediaType }),
      logicalName,
    );
  }
  state.datasetRelease = await api(
    `/workspaces/${encodeURIComponent(state.workspace.id)}/datasets`,
    { method: "POST", body: form },
  );
  assert.equal(state.datasetRelease.state, "published");
  assert.equal(state.datasetRelease.workspace_id, state.workspace.id);
  assert.equal(state.datasetRelease.files.length, indonesiaFileContract.length);
  assert.match(state.datasetRelease.content_sha256, /^[0-9a-f]{64}$/);
  const listed = await api(
    `/workspaces/${encodeURIComponent(state.workspace.id)}/datasets`,
  );
  assert(
    listed.some(
      (release) =>
        release.id === state.datasetRelease.id &&
        release.content_sha256 === state.datasetRelease.content_sha256,
    ),
  );
  return `${state.datasetRelease.dataset_id}@${state.datasetRelease.version}; 10 files`;
});

await runCase("Web-published least-privilege Agent Releases", async () => {
  state.agentReleases.data = await publishAgentRelease("data", [
    state.datasetRelease.id,
  ]);
  state.agentReleases.network = await publishAgentRelease("network", []);
  state.agentReleases.visualization = await publishAgentRelease("visualization", []);

  const definitions = await api("/agent-definitions");
  for (const [kind, published] of Object.entries(state.agentReleases)) {
    const definition = definitions.find(
      (entry) =>
        entry.definition_id === customAgentIds[kind] &&
        entry.version === customVersion &&
        entry.release_id === published.release.id,
    );
    assert(definition, `${customAgentIds[kind]} was not discoverable after publication`);
    assert.equal(definition.source, "user_release");
    if (kind === "data") {
      assert.equal(definition.required_workspace_id, state.workspace.id);
      assert.deepEqual(
        definition.dataset_releases.map((release) => release.release_id),
        [state.datasetRelease.id],
      );
    } else {
      assert.equal(definition.dataset_releases.length, 0);
    }
  }
  return "Data, Network, and Visualization Agent Releases published";
});

await runCase("Web-published dynamic Supervisor Release", async () => {
  const repository = state.repositorySupervisor;
  let customInstructions = repository.custom_instructions;
  for (const [kind, repositoryAgent] of Object.entries(repositoryAgents)) {
    customInstructions = customInstructions.replaceAll(
      identity(repositoryAgent),
      `${customAgentIds[kind]}@${customVersion}`,
    );
  }
  const draft = {
    policy_id: customPolicyId,
    version: customVersion,
    display_name: "Indonesia Network Tutorial Supervisor",
    description:
      "Dynamically coordinates the smallest governed Agent set needed for an Indonesia network decision.",
    responsibilities: repository.responsibilities,
    instruction_policy: {
      policy_id: repository.instruction_policy.policy_id,
      version: repository.instruction_policy.version,
    },
    custom_instructions: customInstructions,
    agents: Object.keys(repositoryAgents).map((kind) => ({
      definition_id: customAgentIds[kind],
      version: customVersion,
      release_id: state.agentReleases[kind].release.id,
      spawn_limit: 1,
    })),
    artifact_contracts: repository.artifact_contracts.map((contract) => ({
      artifact_type: contract.artifact_type,
      producer_agent: remapAgentIdentity(contract.producer_agent),
      consumer_agents: contract.consumer_agents.map(remapAgentIdentity),
      required: contract.required,
    })),
    max_active_child_agents: 3,
  };
  const resource = await api("/supervisor-definitions", {
    method: "POST",
    body: draft,
  });
  const validation = await api(
    `/supervisor-definitions/${encodeURIComponent(resource.id)}/validate`,
    { method: "POST" },
  );
  assert.equal(validation.valid, true, JSON.stringify(validation.issues));
  assert.match(validation.content_sha256, /^[0-9a-f]{64}$/);
  assert.match(validation.execution_semantics_sha256, /^[0-9a-f]{64}$/);
  const release = await api(
    `/supervisor-definitions/${encodeURIComponent(resource.id)}/publish`,
    { method: "POST" },
  );
  state.supervisorRelease = { draft, resource, release };
  state.policy = { policy_id: customPolicyId, version: customVersion };
  const resolved = await api(
    `/supervisor-policies/${customPolicyId}/${customVersion}`,
  );
  assert.equal(resolved.source, "user_release");
  assert.deepEqual(
    resolved.agents.map((entry) => entry.release_id).sort(),
    Object.values(state.agentReleases).map((entry) => entry.release.id).sort(),
  );
  assert.equal(resolved.max_active_child_agents, 3);
  return `${customPolicyId}@${customVersion}`;
});

await runCase("Policy-bound root Thread", async () => {
  state.task = await api("/tasks", {
    method: "POST",
    body: {
      project_id: state.project.id,
      title: "印尼全国履约网络决策",
      model_provider: providerId,
      model,
    },
  });
  const started = await api(`/tasks/${state.task.id}/runs`, {
    method: "POST",
    body: {
      idempotency_key: `indonesia-run-${crypto.randomUUID()}`,
      workspace_id: state.workspace.id,
      fork_thread_id: null,
      fork_source_run_id: null,
      supervisor_policy: state.policy,
    },
  });
  state.run = await eventually(async () => {
    const run = await api(`/runs/${started.run.id}`);
    return run.codex_thread_id && run.workspace_id ? run : undefined;
  }, "Policy-bound root Thread", 120_000, 500);
  const binding = await eventually(async () => {
    const current = await api(`/runs/${state.run.id}/supervisor-policy`);
    return current?.state === "bound" ? current : undefined;
  }, "Supervisor Policy binding");
  assert.equal(binding.thread_id, state.run.codex_thread_id);
  assert.equal(binding.policy_id, state.policy.policy_id);
  assert.equal(binding.version, state.policy.version);
  return `run=${state.run.id}; root Thread=${state.run.codex_thread_id}`;
});

await runCase("dynamic Indonesian network decision and map", async () => {
  const prompt = promptOverride ?? `分析现有印尼仓库网络并给出建议。

我需要知道当前网络的一日、两日和三日需求覆盖率、全网干线与末端年度运输成本、各省时效表现，并按数据合同声明的省份排名口径分别列出表现最好和最需要改善的前三个省份。然后判断能否在完整的已审核候选点中新增一个前置仓，使两日需求覆盖率达到 74%；建设费用按 5 年摊销。请比较当前方案与入选方案的一日、两日、三日覆盖率，以及干线、末端、运输总成本、年度固定成本、摊销建设成本和年度决策总成本，并创建一张地图展示两者差异。

请根据尚未解决的证据问题动态决定需要哪些 Agent，不要为了凑数量运行角色，也不要按写死的 Agent 顺序执行。只使用平台授权的数据与类型化 Artifact；不要扫描工作区、运行终端命令、读取原始客户行、调用导航服务或自行编造候选地点。距离采用教程声明的球面距离乘系数方法，司机每天可行驶 6 小时。

最终报告先给出“证据索引”，再使用“事实、假设、分析、建议、局限、缺失证据”六个部分。证据索引必须列出数据检查、现网、候选优化、入选方案和地图所使用的每个原样 resource_name 及其负责的事实。每个关键数字注明拥有该字段的 Artifact schema 和原样 resource_name，不要用服务基线引用成本，不要用入选方案引用完整候选集合，也不要暴露 Resource URI。优化状态必须原样写出 Tool 返回的枚举；所有金额保留 Resource 中的精确 IDR 整数，不要换算为 B、million、billion、万或亿，也不要自行计算新的金额比例。删除任何没有精确前后字段支撑的运营效果推断。如果已生成地图，必须原样嵌入地图交付指令。`;

  const sent = await api(`/tasks/${state.task.id}/messages`, {
    method: "POST",
    body: {
      text: prompt,
      model,
      model_provider: providerId,
      effort,
      service_tier: null,
      access_mode: "workspace-write",
      images: [],
      collaboration_mode: null,
    },
  });
  assert.equal(sent.thread_id, state.run.codex_thread_id);
  state.turnId = sent.turn_id;
  const events = await waitForTurn(state.task.id, state.turnId);
  return `${events.length} durable browser events`;
});

if (observationOnly) {
  await runCase("capture Runtime collaboration evidence", async () => {
    const [agents, executions, events, artifacts, binding] = await Promise.all([
      api(`/runs/${state.run.id}/agents`),
      api(`/runs/${state.run.id}/agent-executions`),
      allTaskEvents(state.task.id),
      api(`/tasks/${state.task.id}/artifacts`),
      api(`/runs/${state.run.id}/supervisor-policy`),
    ]);
    const calls = completedMcpCalls(events);
    const report = lastFinalReport(events);
    assert(report, "Root Supervisor did not produce a final report");
    if (evidenceFile) {
      const evidence = {
        schemaVersion: "indonesia-supervisor-observation.v1",
        verifiedAt: new Date().toISOString(),
        case: caseName,
        provider: { id: providerId, model },
        datasetRelease: state.datasetRelease,
        agentReleases: Object.fromEntries(
          Object.entries(state.agentReleases).map(([kind, value]) => [
            kind,
            value.release,
          ]),
        ),
        policy: binding,
        run: {
          id: state.run.id,
          rootThreadId: state.run.codex_thread_id,
          turnId: state.turnId,
          workspaceId: state.workspace.id,
        },
        agents,
        executions,
        toolCalls: calls.map((event) => ({
          sequence: event.sequence,
          threadId: event.thread_id,
          server: event.payload?.data?.server,
          tool: event.payload?.data?.tool,
        })),
        artifacts,
        report,
      };
      await mkdir(path.dirname(evidenceFile), { recursive: true });
      await writeFile(evidenceFile, `${JSON.stringify(evidence, null, 2)}\n`, {
        mode: 0o600,
      });
      log(`\nEvidence: ${evidenceFile}`);
    }
    return `${agents.length} Runtime Threads; ${executions.length} Agent tasks; ${calls.length} MCP calls; ${artifacts.length} Resource Artifacts`;
  });

  log("\nIndonesia Supervisor observation summary");
  for (const result of results) {
    log(`- ${result.status.toUpperCase()} ${result.name} (${result.durationMs} ms)`);
  }
  log(`\n${results.length}/${results.length} cases passed.`);
  process.exit(0);
}

let finalEvidence;
await runCase("Runtime-selected Agent tree and governed tool scope", async () => {
  const agents = await eventually(async () => {
    const current = await api(`/runs/${state.run.id}/agents`);
    return current.length >= 4 ? current : undefined;
  }, "root plus three task-required Runtime child Threads");
  const root = agents.find((agent) => agent.is_root);
  assert(root);
  assert.equal(root.agent_role, null);
  assert.equal(root.thread_id, state.run.codex_thread_id);
  const children = agents.filter((agent) => !agent.is_root);
  assert.equal(children.length, 3, "Supervisor did not choose the three task-required roles");
  assert(
    children.every((agent) => agent.parent_thread_id === root.thread_id),
    "A governed child Agent is not attached to the root Thread",
  );
  assert(
    children.every(
      (agent) => !["failed", "interrupted"].includes(agent.status_type),
    ),
    "A selected Agent terminated unsuccessfully",
  );

  const events = await allTaskEvents(state.task.id);
  const calls = completedMcpCalls(events);
  const inspectCall = findToolCall(
    calls,
    "supply_chain_indonesia",
    "inspect_indonesia_dataset_release",
  );
  const currentCall = findToolCall(
    calls,
    "supply_chain_indonesia",
    "evaluate_indonesia_current_network",
  );
  const optimizeCall = findToolCall(
    calls,
    "supply_chain_indonesia",
    "optimize_indonesia_new_warehouse",
  );
  const prepareMapCall = findToolCall(
    calls,
    "supply_chain_indonesia",
    "prepare_indonesia_network_map",
  );
  const prepareRenderCall = findToolCall(
    calls,
    "supply_chain_indonesia",
    "prepare_indonesia_map_render",
  );
  const createMapCall = findToolCall(calls, "map_utils", "create_map_card");
  const prepareReportCall = findToolCall(
    calls,
    "supply_chain_indonesia",
    "prepare_indonesia_decision_report",
  );
  assert(inspectCall, "Data Agent did not inspect the authorized Dataset Release");
  assert(currentCall, "Network Agent did not evaluate the current network");
  assert(optimizeCall, "Network Agent did not evaluate the finite candidate set");
  assert(prepareMapCall, "Network Agent did not prepare bounded map evidence");
  assert(
    prepareRenderCall,
    "Visualization Agent did not resolve the exact map Resource names",
  );
  assert(createMapCall, "Visualization Agent did not create a browser map");
  assert(
    prepareReportCall,
    "Network Agent did not prepare the deterministic decision report",
  );
  assert.equal(
    calls.filter(
      (event) =>
        event.payload?.data?.server === "supply_chain_indonesia" &&
        event.payload?.data?.tool === "prepare_indonesia_decision_report",
    ).length,
    1,
    "Network Agent retried or duplicated deterministic report publication",
  );

  const dataAgent = agents.find((agent) => agent.thread_id === inspectCall.thread_id);
  const networkAgent = agents.find((agent) => agent.thread_id === currentCall.thread_id);
  const visualizationAgent = agents.find(
    (agent) => agent.thread_id === createMapCall.thread_id,
  );
  assert(dataAgent && networkAgent && visualizationAgent);
  assert.notEqual(dataAgent.thread_id, networkAgent.thread_id);
  assert.notEqual(networkAgent.thread_id, visualizationAgent.thread_id);
  assert.notEqual(dataAgent.thread_id, visualizationAgent.thread_id);
  assert.equal(optimizeCall.thread_id, networkAgent.thread_id);
  assert.equal(prepareMapCall.thread_id, networkAgent.thread_id);
  assert.equal(prepareReportCall.thread_id, networkAgent.thread_id);
  assert.equal(prepareRenderCall.thread_id, visualizationAgent.thread_id);

  const visualizationHandoff = events
    .filter(
      (event) =>
        event.thread_id === visualizationAgent.thread_id &&
        event.event_type === "codex.item.completed" &&
        itemType(event) === "agentMessage",
    )
    .map((event) => event.payload?.data?.text)
    .filter((text) => typeof text === "string" && text.includes("MAP_HANDOFF"))
    .at(-1);
  assert(visualizationHandoff, "Visualization Agent omitted MAP_HANDOFF provenance");
  assert(
    /"map_manifest_resource_name"\s*:\s*"indonesia_network_map\.v1-[a-f0-9]{24}"/i.test(
      visualizationHandoff,
    ),
    "Visualization Agent omitted the exact map manifest Resource name",
  );
  assert(
    /"geojson_resource_name"\s*:\s*"geojson\.v1-[a-f0-9]{24}"/i.test(
      visualizationHandoff,
    ),
    "Visualization Agent omitted the exact GeoJSON Resource name",
  );
  assert(
    /"map_artifact_id"\s*:\s*"map-[a-f0-9-]+"/i.test(visualizationHandoff),
    "Visualization Agent omitted the map Artifact ID",
  );
  const mapArtifactId = visualizationHandoff.match(
    /"map_artifact_id"\s*:\s*"(map-[a-f0-9-]+)"/i,
  )?.[1];
  const mapEmbedArtifactId = visualizationHandoff.match(
    /"map_embed_code"\s*:\s*"::codex-inline-vis\{artifact=\\"(map-[a-f0-9-]+)\\"\}"/i,
  )?.[1];
  assert(
    mapEmbedArtifactId,
    "Visualization Agent omitted the exact map embed code from MAP_HANDOFF",
  );
  assert.equal(
    mapEmbedArtifactId,
    mapArtifactId,
    "Visualization Agent map Artifact ID and embed code diverged",
  );
  const mapEmbedCode = `::codex-inline-vis{artifact="${mapArtifactId}"}`;

  const executions = await eventually(async () => {
    const current = await api(`/runs/${state.run.id}/agent-executions`);
    return current.length >= 3 &&
      current.every((execution) =>
        ["completed", "failed", "interrupted"].includes(execution.status)
      )
      ? current
      : undefined;
  }, "persisted child Agent task executions");
  assert(executions.every((execution) => execution.status === "completed"));
  for (const agent of [dataAgent, networkAgent, visualizationAgent]) {
    assert(
      executions.some((execution) => execution.thread_id === agent.thread_id),
      `Agent ${agent.thread_id} has no persisted execution`,
    );
  }
  assert(
    executions.every(
      (execution) =>
        typeof execution.task === "string" && execution.task.trim().length > 0,
    ),
    "Agent task text was not persisted",
  );

  const commandExecutions = events.filter(
    (event) => itemType(event) === "commandExecution",
  );
  assert.equal(
    commandExecutions.length,
    0,
    "A governed Agent bypassed Runtime capabilities with a terminal command",
  );
  assert.equal(
    calls.filter((event) => event.thread_id === root.thread_id).length,
    0,
    "Root Supervisor executed a business MCP Tool",
  );
  const failedMcpCalls = calls.filter(
    (event) => event.payload?.data?.status !== "completed",
  );
  assert.equal(
    failedMcpCalls.length,
    0,
    `Governed Agents made failed MCP calls: ${failedMcpCalls
      .map(
        (event) =>
          `${event.payload?.data?.server}.${event.payload?.data?.tool}: ${event.payload?.data?.error?.message ?? "unknown error"}`,
      )
      .join("; ")}`,
  );
  assert.equal(
    calls.filter((event) =>
      ["list_mcp_resources", "list_mcp_resource_templates"].includes(
        event.payload?.data?.tool,
      )
    ).length,
    0,
    "An Agent searched MCP Resource catalogs instead of using the exact Artifact handoff",
  );

  const resourceProtocolTools = new Set([
    "read_mcp_resource",
  ]);
  const allowedBusinessTools = new Map([
    [
      dataAgent.thread_id,
      new Set(["inspect_indonesia_dataset_release"]),
    ],
    [
      networkAgent.thread_id,
      new Set([
        "evaluate_indonesia_service_baseline",
        "evaluate_indonesia_current_network",
        "evaluate_indonesia_candidate",
        "optimize_indonesia_new_warehouse",
        "prepare_indonesia_network_map",
        "prepare_indonesia_decision_report",
        "validate_indonesia_resource",
      ]),
    ],
    [
      visualizationAgent.thread_id,
      new Set(["prepare_indonesia_map_render", "create_map_card"]),
    ],
  ]);
  for (const call of calls) {
    const tool = call.payload?.data?.tool;
    const server = call.payload?.data?.server;
    if (resourceProtocolTools.has(tool)) continue;
    const allowed = allowedBusinessTools.get(call.thread_id);
    assert(allowed?.has(tool), `Tool ${server}.${tool} escaped its Agent role`);
    assert(
      (server === "supply_chain_indonesia" && tool !== "create_map_card") ||
        (server === "map_utils" && tool === "create_map_card"),
      `Tool ${server}.${tool} used an undeclared MCP server`,
    );
  }
  assert(
    !calls.some(
      (event) =>
        event.thread_id === visualizationAgent.thread_id &&
        resourceProtocolTools.has(event.payload?.data?.tool),
    ),
    "Visualization Agent bypassed typed map preparation with direct Resource reads",
  );
  assert(inspectCall.sequence < currentCall.sequence);
  assert(currentCall.sequence < prepareMapCall.sequence);
  assert(optimizeCall.sequence < prepareMapCall.sequence);
  assert(prepareMapCall.sequence < prepareRenderCall.sequence);
  assert(prepareRenderCall.sequence < createMapCall.sequence);
  assert(prepareMapCall.sequence < prepareReportCall.sequence);

  finalEvidence = {
    agents,
    executions,
    events,
    calls,
    dataAgent,
    networkAgent,
    visualizationAgent,
    mapEmbedCode,
  };
  return `root + 3 child Threads; ${executions.length} persisted Agent tasks; ${calls.length} MCP calls`;
});

await runCase("durable typed Resources and cross-Agent map resolution", async () => {
  const requiredSchemas = [
    "indonesia_dataset_inspection.v1",
    "indonesia_service_baseline.v1",
    "indonesia_current_network_analysis.v1",
    "indonesia_candidate_scenario.v1",
    "indonesia_location_optimization.v1",
    "indonesia_network_map.v1",
    "geojson.v1",
    "indonesia_decision_report.v1",
  ];
  const optionalSchemas = [];
  const artifacts = await eventually(async () => {
    const current = await api(`/tasks/${state.task.id}/artifacts`);
    const failed = current.find((artifact) => artifact.state === "failed");
    if (failed) throw new Error(`Artifact ${failed.id} failed materialization`);
    const schemas = new Set(current.map((artifact) => artifact.artifact_schema));
    return requiredSchemas.every((schema) => schemas.has(schema)) &&
      current.every((artifact) => artifact.state === "ready")
      ? current
      : undefined;
  }, "durable Indonesia Resource Artifacts", 180_000, 1_000);

  for (const schema of requiredSchemas) {
    assert.equal(
      artifacts.filter((artifact) => artifact.artifact_schema === schema).length,
      1,
      `${schema} did not materialize exactly once`,
    );
  }
  for (const schema of optionalSchemas) {
    assert(
      artifacts.filter((artifact) => artifact.artifact_schema === schema).length <= 1,
      `${schema} materialized more than once`,
    );
  }
  const inspection = artifacts.find(
    (artifact) => artifact.artifact_schema === "indonesia_dataset_inspection.v1",
  );
  assert.equal(inspection.producer_agent_role, finalEvidence.dataAgent.agent_role);
  for (const artifact of artifacts.filter(
    (entry) => entry.artifact_schema !== "indonesia_dataset_inspection.v1",
  )) {
    assert.equal(artifact.producer_agent_role, finalEvidence.networkAgent.agent_role);
  }

  const contents = {};
  for (const artifact of artifacts) {
    const summary = await api(`/artifacts/${artifact.id}`);
    assert.equal(summary.id, artifact.id);
    assert(!Object.hasOwn(summary, "source_server"));
    assert(!Object.hasOwn(summary, "source_uri"));
    const content = await api(`/artifacts/${artifact.id}/content`);
    if (artifact.artifact_schema === "geojson.v1") {
      assert.equal(content.type, "FeatureCollection");
      assert(content.features.length > 0 && content.features.length < 200);
      assert(
        content.features.every(
          (feature) => !Object.hasOwn(feature.properties ?? {}, "customer_id"),
        ),
        "Map Artifact exposed customer rows",
      );
    } else {
      assert.equal(content.schema_version, artifact.artifact_schema);
    }
    contents[artifact.id] = content;
  }

  const mapReportEvent = finalReportEvent(finalEvidence.events);
  assert(mapReportEvent, "Root report did not restore the Visualization Agent map");
  const inlineMaps = mapReportEvent.payload.data.inlineArtifacts.filter(
    (artifact) => artifact.renderer?.kind === "map.v3",
  );
  assert.equal(inlineMaps.length, 1);
  const rendererPayload = inlineMaps[0].renderer.payload;
  assert.equal(rendererPayload.status, "ready");
  assert.equal(rendererPayload.sources.network.data.type, "artifact");
  assert.match(
    rendererPayload.sources.network.data.url,
    /^\/api\/artifacts\/[0-9a-f-]+\/content$/,
  );
  assert(!JSON.stringify(rendererPayload).includes("supply-chain-indonesia://"));

  const binding = await api(`/runs/${state.run.id}/supervisor-policy`);
  const safeTrace = JSON.stringify({
    binding,
    agents: finalEvidence.agents,
    executions: finalEvidence.executions,
    artifacts,
    events: finalEvidence.events,
  });
  assert(!safeTrace.includes("supply-chain-indonesia://"));
  assert(!safeTrace.includes(repoRoot));

  finalEvidence.artifacts = artifacts;
  finalEvidence.contents = contents;
  finalEvidence.binding = binding;
  finalEvidence.inlineMap = inlineMaps[0];
  return `${artifacts.length} ready Resource Artifacts; one restored map.v3 Artifact`;
});

await runCase("evidence-backed Supervisor report", async () => {
  const report = lastFinalReport(finalEvidence.events);
  assert(report, "Root Supervisor did not produce a final report");
  const decisionReportArtifact = finalEvidence.artifacts.find(
    (artifact) => artifact.artifact_schema === "indonesia_decision_report.v1",
  );
  assert(decisionReportArtifact, "Deterministic decision-report Artifact is missing");
  const decisionReport = finalEvidence.contents[decisionReportArtifact.id];
  const expectedReport = `${decisionReport.markdown.trim()}\n\n${finalEvidence.mapEmbedCode}`;
  assert.equal(
    report.trim(),
    expectedReport,
    "Root Supervisor rewrote the Tool-owned report or failed to append the exact map embed",
  );
  assert(
    !decisionReport.markdown.includes("::codex-inline-vis{"),
    "Decision-report Resource crossed ownership by embedding a browser Artifact",
  );
  assert(
    hasSection(report, "证据索引", "Evidence Index"),
    "Final report omitted the evidence ownership index",
  );
  for (const [chinese, english] of reportSections) {
    assert(hasSection(report, chinese, english), `Final report omitted ${chinese}`);
  }
  const reportSchemas = [
    "indonesia_dataset_inspection.v1",
    "indonesia_current_network_analysis.v1",
    "indonesia_location_optimization.v1",
    "indonesia_candidate_scenario.v1",
    "indonesia_network_map.v1",
    "geojson.v1",
  ];
  if (
    finalEvidence.artifacts.some(
      (artifact) => artifact.artifact_schema === "indonesia_service_baseline.v1",
    )
  ) {
    reportSchemas.splice(1, 0, "indonesia_service_baseline.v1");
  }
  for (const schema of reportSchemas) {
    const escaped = schema.replaceAll(".", "\\.");
    assert(
      new RegExp(`${escaped}-[a-f0-9]{24}`, "i").test(report),
      `Final report did not cite ${schema} by exact Resource name`,
    );
  }
  assert(/%/.test(report), "Final report omitted coverage percentages");
  assert(/IDR/i.test(report), "Final report omitted the cost currency");
  assert(
    /74(?:\.0+)?%/.test(report),
    "Final report did not address the two-day coverage target",
  );

  const artifactContent = (schema) => {
    const artifact = finalEvidence.artifacts.find(
      (entry) => entry.artifact_schema === schema,
    );
    assert(artifact, `Missing verified ${schema} Artifact`);
    return finalEvidence.contents[artifact.id];
  };
  const current = artifactContent("indonesia_current_network_analysis.v1");
  const scenario = artifactContent("indonesia_candidate_scenario.v1");
  const optimization = artifactContent("indonesia_location_optimization.v1");

  for (const day of ["1_day", "2_day", "3_day"]) {
    assert(
      report.includes(percentage(current.coverage.demand_coverage[day])),
      `Final report omitted exact current ${day} demand coverage`,
    );
    assert(
      report.includes(percentage(scenario.candidate_coverage.demand_coverage[day])),
      `Final report omitted exact selected-scenario ${day} demand coverage`,
    );
  }
  for (const [label, value] of [
    ["current linehaul cost", current.costs.linehaul_idr],
    ["current last-mile cost", current.costs.last_mile_idr],
    ["current transport cost", current.costs.transport_total_idr],
    ["selected linehaul cost", scenario.candidate_costs.linehaul_idr],
    ["selected last-mile cost", scenario.candidate_costs.last_mile_idr],
    ["selected transport cost", scenario.candidate_costs.transport_total_idr],
    ["selected annual fixed cost", scenario.candidate_costs.annual_fixed_cost_idr],
    [
      "selected annualized opening cost",
      scenario.candidate_costs.annualized_opening_cost_idr,
    ],
    ["selected annual decision cost", scenario.candidate_costs.annual_decision_cost_idr],
  ]) {
    assert(
      reportHasInteger(report, value),
      `Final report omitted exact ${label}: ${value}`,
    );
  }
  const provincesByCode = new Map(
    current.provinces.map((province) => [province.province_code, province]),
  );
  for (const [kind, codes] of [
    ["priority", current.priority_province_codes.slice(0, 3)],
    ["best", current.best_province_codes.slice(0, 3)],
  ]) {
    for (const code of codes) {
      const province = provincesByCode.get(code);
      assert(province, `Current network Artifact omitted province ${code}`);
      assert(
        report.includes(code) || report.includes(province.province_name),
        `Final report omitted ${kind} province ${province.province_name} (${code})`,
      );
    }
    assert(
      rankedProvincePattern(codes, provincesByCode).test(report),
      `Final report did not preserve the authoritative ${kind} province order`,
    );
  }

  const allowedOperationalIds = new Set([
    ...current.warehouses.map((warehouse) => warehouse.warehouse_id),
    ...scenario.warehouses.map((warehouse) => warehouse.warehouse_id),
    scenario.candidate.candidate_id,
    ...optimization.evaluations.map((candidate) => candidate.candidate_id),
  ]);
  const namedOperationalIds = new Set(
    report.match(/\b(?:CEN|FWD|CAN)-[A-Z0-9-]+\b/g) ?? [],
  );
  for (const operationalId of namedOperationalIds) {
    assert(
      allowedOperationalIds.has(operationalId),
      `Final report named an operational entity absent from validated Resources: ${operationalId}`,
    );
  }

  const allowedProvinceCodes = new Set(current.provinces.map((province) => province.province_code));
  const namedProvinceCodes = new Set(report.match(/\bIDN\d{3}\b/g) ?? []);
  for (const provinceCode of namedProvinceCodes) {
    assert(
      allowedProvinceCodes.has(provinceCode),
      `Final report named a province absent from the current-network Resource: ${provinceCode}`,
    );
  }

  const qualifyingCandidates = optimization.evaluations.filter(
    (candidate) => candidate.target_met,
  );
  assert.equal(
    optimization.target_met_candidate_count,
    qualifyingCandidates.length,
    "Optimization Resource qualifying count does not match its evaluations",
  );
  assert(qualifyingCandidates.length > 1, "Fixture must retain multiple qualifying candidates");
  assert(
    new RegExp(
      `(?:${optimization.target_met_candidate_count}\\s*个[^\\n]{0,24}(?:达标|候选)|` +
        `${optimization.target_met_candidate_count}\\s+(?:qualifying|feasible|target-meeting)[^\\n]{0,12}candidates?)`,
      "i",
    ).test(report),
    "Final report omitted the exact number of target-meeting candidates",
  );
  assert(
    /(?:年度决策(?:总)?成本[^。\n]{0,30}(?:最低|最小)|(?:最低|最小化?)[^。\n]{0,30}年度决策(?:总)?成本|(?:minimi[sz]e|lowest)[^.\n]{0,30}annual decision cost)/i.test(
      report,
    ),
    "Final report did not state the objective that selected the candidate",
  );
  assert(
    report.includes(optimization.status),
    `Final report did not preserve optimization status ${optimization.status}`,
  );
  if (optimization.status === "target_met") {
    assert(
      !/target_already_met\s*(?:=|:|：|is)?\s*(?:true|是)/i.test(report),
      "Final report rewrote target_met as target_already_met",
    );
  }
  assert(
    !/(?:唯一[^。\n]{0,30}(?:达标|超过[^。\n]{0,8}目标|满足[^。\n]{0,8}目标)|only (?:candidate|one)[^.\n]{0,30}(?:meet|exceed)[^.\n]{0,16}target)/i.test(
      report,
    ),
    "Final report falsely claimed the selected candidate was uniquely feasible",
  );
  assert(
    !/(?:有望|可能|预计)[^。\n]{0,32}(?:缓解|减轻|分担|转移)[^。\n]{0,20}(?:压力|负荷|工作量)|(?:may|might|likely|expected to)[^.\n]{0,48}(?:relieve|reduce pressure|shift workload)/i.test(
      report,
    ),
    "Final report retained an unsupported operational-effect inference",
  );
  assert(
    !/(?:IDR[^。\n]{0,28}(?:万亿|亿元?|百万|十亿|million|billion|\b[BM]\b)|(?:成本|费用|金额|支出|节约)[^。\n]{0,36}\d[\d,.]*\s*(?:万亿|亿元?|百万|十亿|million|billion|\b[BM]\b))/i.test(
      report,
    ),
    "Final report introduced an unverified monetary unit conversion",
  );
  assert(!report.includes("57.5%"));
  assert(!report.includes("42.5%"));
  assert(!report.includes("1.33个百分点"));
  assert(!report.includes("卸下"));
  assert(
    report.trim().endsWith(finalEvidence.mapEmbedCode),
    "Final report omitted the exact map embed directive",
  );
  assert(!report.includes("[internal-resource-uri]"));
  assert(!report.includes("supply-chain-indonesia://"));
  assert(!report.includes(repoRoot));
  finalEvidence.report = report;
  return `${report.length} characters with six decision sections and a map`;
});

await runCase("browser history and evidence recovery", async () => {
  const turns = await api(`/runs/${state.run.id}/thread/turns`);
  const reportItems = turns
    .flatMap((turn) => turn.items ?? [])
    .filter((item) => item.type === "agentMessage" && typeof item.text === "string")
    .filter((item) =>
      reportSections.every(([chinese, english]) =>
        hasSection(item.text, chinese, english),
      ),
    );
  assert(reportItems.length > 0, "Browser history did not restore the final report");
  const restoredReport = reportItems.at(-1);
  assert.equal(restoredReport.text, finalEvidence.report);
  assert(
    restoredReport.inlineArtifacts?.some(
      (artifact) => artifact.renderer?.kind === "map.v3",
    ),
    "Browser history did not restore the map Artifact",
  );

  const [binding, agents, executions, artifacts] = await Promise.all([
    api(`/runs/${state.run.id}/supervisor-policy`),
    api(`/runs/${state.run.id}/agents`),
    api(`/runs/${state.run.id}/agent-executions`),
    api(`/tasks/${state.task.id}/artifacts`),
  ]);
  const overview = { binding, agents, executions, artifacts };
  assert.equal(binding.policy_id, state.policy.policy_id);
  assert.equal(binding.version, state.policy.version);
  assert.equal(agents.length, finalEvidence.agents.length);
  assert.deepEqual(executions, finalEvidence.executions);
  assert.equal(artifacts.length, finalEvidence.artifacts.length);
  assert(artifacts.every((artifact) => artifact.state === "ready"));
  assert(!JSON.stringify(overview).includes("supply-chain-indonesia://"));
  return `${turns.length} restored Turns; ${agents.length} Agents; ${artifacts.length} Resource Artifacts`;
});

if (lifecycleProbe) {
  await runCase("completed Network Agent follow-up preserves child Thread identity", async () => {
    const target = finalEvidence.networkAgent;
    const previousExecutions = finalEvidence.executions
      .filter((execution) => execution.thread_id === target.thread_id)
      .sort((left, right) => left.ordinal - right.ordinal);
    const previousOrdinal = previousExecutions.at(-1).ordinal;
    const sent = await api(`/tasks/${state.task.id}/messages`, {
      method: "POST",
      body: {
        text: `执行一次生命周期复核。不要创建任何新 Agent。必须通过 Runtime 协作工具向上一轮已完成的 Network Planning Agent 发送一个新任务，请它基于已验证证据说明“当前入选方案相对实际网络的主要改善，以及什么变化会推翻该结论”，等待同一个 Agent 完成后再用两句话汇总。不要重新运行 MCP 工具。`,
        model,
        model_provider: providerId,
        effort,
        service_tier: null,
        access_mode: "workspace-write",
        images: [],
        collaboration_mode: null,
      },
    });
    await waitForTurn(state.task.id, sent.turn_id);

    const targetExecutions = await eventually(async () => {
      const current = await api(`/runs/${state.run.id}/agent-executions`);
      const matching = current
        .filter((execution) => execution.thread_id === target.thread_id)
        .sort((left, right) => left.ordinal - right.ordinal);
      const latest = matching.at(-1);
      return latest?.ordinal === previousOrdinal + 1 && latest.status === "completed"
        ? matching
        : undefined;
    }, "follow-up execution on the completed Network Agent", 300_000, 1_000);
    const agents = await api(`/runs/${state.run.id}/agents`);
    assert.equal(agents.length, finalEvidence.agents.length, "Follow-up spawned a new Agent");
    const followUp = targetExecutions.at(-1);
    const turns = await api(
      `/runs/${state.run.id}/agents/${encodeURIComponent(target.thread_id)}/turns`,
    );
    assert(
      turns.some((turn) => turn.id === followUp.turn_id),
      "Browser-safe child history omitted the follow-up Turn",
    );
    finalEvidence.lifecycle = {
      followUp: {
        rootTurnId: sent.turn_id,
        agentThreadId: target.thread_id,
        agentTurnId: followUp.turn_id,
        ordinal: followUp.ordinal,
      },
    };
    return `same child Thread=${target.thread_id}; execution ordinal=${followUp.ordinal}`;
  });

  await runCase("Runtime interrupt reaches a terminal outcome and recovers", async () => {
    const interrupted = await api(`/tasks/${state.task.id}/messages`, {
      method: "POST",
      body: {
        text: "开始一次新的网络假设复核，在给出结论前逐项检查上一轮证据。",
        model,
        model_provider: providerId,
        effort,
        service_tier: null,
        access_mode: "workspace-write",
        images: [],
        collaboration_mode: null,
      },
    });
    await waitForTurnStarted(state.task.id, interrupted.turn_id);
    const interruptResult = await api(`/runs/${state.run.id}/interrupt`, {
      method: "POST",
      body: { turn_id: interrupted.turn_id },
    });
    assert.equal(interruptResult.status, "interrupted");
    const interruptedTurn = await eventually(async () => {
      const turns = await api(`/runs/${state.run.id}/thread/turns`);
      const turn = turns.find((candidate) => candidate.id === interrupted.turn_id);
      return turn?.status?.toLowerCase().includes("interrupt") ? turn : undefined;
    }, "authoritative interrupted Turn history", 120_000, 500);

    const recovered = await api(`/tasks/${state.task.id}/messages`, {
      method: "POST",
      body: {
        text: "不要创建 Agent 或调用工具，只回复 LIFECYCLE_RECOVERED。",
        model,
        model_provider: providerId,
        effort,
        service_tier: null,
        access_mode: "workspace-write",
        images: [],
        collaboration_mode: null,
      },
    });
    await waitForTurn(state.task.id, recovered.turn_id);
    const rootTurns = await api(`/runs/${state.run.id}/thread/turns`);
    const recoveredTurn = rootTurns.find((turn) => turn.id === recovered.turn_id);
    assert(recoveredTurn, "Runtime did not persist the recovery Turn");
    assert(
      recoveredTurn.items.some(
        (item) =>
          item.type === "agentMessage" &&
          typeof item.text === "string" &&
          item.text.includes("LIFECYCLE_RECOVERED"),
      ),
      "Recovery Turn did not complete with the expected response",
    );
    finalEvidence.lifecycle.interrupt = {
      interruptedTurnId: interruptedTurn.id,
      interruptedStatus: interruptedTurn.status,
      recoveryTurnId: recovered.turn_id,
    };
    return `interrupted=${interruptedTurn.status}; recovery Turn=${recovered.turn_id}`;
  });
}

if (evidenceFile) {
  const evidence = {
    schemaVersion: "indonesia-supervisor-e2e.v1",
    verifiedAt: new Date().toISOString(),
    case: "印尼全国履约网络决策",
    provider: { id: providerId, model },
    datasetRelease: state.datasetRelease,
    agentReleases: Object.fromEntries(
      Object.entries(state.agentReleases).map(([kind, value]) => [
        kind,
        value.release,
      ]),
    ),
    policy: finalEvidence.binding,
    run: {
      id: state.run.id,
      rootThreadId: state.run.codex_thread_id,
      turnId: state.turnId,
      workspaceId: state.workspace.id,
    },
    agents: finalEvidence.agents,
    executions: finalEvidence.executions,
    toolCalls: finalEvidence.calls.map((event) => ({
      sequence: event.sequence,
      threadId: event.thread_id,
      server: event.payload?.data?.server,
      tool: event.payload?.data?.tool,
    })),
    artifacts: finalEvidence.artifacts,
    inlineMap: finalEvidence.inlineMap,
    report: finalEvidence.report,
    ...(finalEvidence.lifecycle ? { lifecycle: finalEvidence.lifecycle } : {}),
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
