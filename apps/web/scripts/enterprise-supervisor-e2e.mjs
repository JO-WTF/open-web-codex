#!/usr/bin/env node

import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const baseUrl = (process.env.E2E_BASE_URL ?? "http://127.0.0.1:4810").replace(/\/$/, "");
const apiBase = `${baseUrl}/api`;
const providerId = process.env.E2E_PROVIDER_ID ?? "deepseek-enterprise-e2e";
const providerBaseUrl = process.env.E2E_PROVIDER_BASE_URL ?? "https://api.deepseek.com";
const providerWireApi = process.env.E2E_PROVIDER_WIRE_API ?? "chat";
const model = process.env.E2E_MODEL ?? "deepseek-v4-flash";
const effort = process.env.E2E_EFFORT ?? "none";
const useBuiltInProvider = process.env.E2E_USE_BUILT_IN_PROVIDER === "1";
const username = process.env.E2E_ADMIN_USERNAME ?? "enterprise-e2e";
const email = process.env.E2E_ADMIN_EMAIL ?? "enterprise-e2e@open-web-codex.local";
const password = process.env.E2E_ADMIN_PASSWORD ?? "open-web-codex-enterprise-e2e";
const policy = {
  policy_id: "enterprise-supervisor-copilot",
  version: "1.0.0",
};
const providerKey = useBuiltInProvider
  ? null
  : await loadSecret(
      "DEEPSEEK_API_KEY",
      process.env.DEEPSEEK_API_KEY_FILE,
    );
const routeFixture = JSON.parse(
  await readFile(
    path.join(
      repoRoot,
      "tools/supply-chain-network-planner/examples/route-matrix-input.json",
    ),
    "utf8",
  ),
);
const evidenceFile = process.env.E2E_EVIDENCE_FILE
  ? path.resolve(process.env.E2E_EVIDENCE_FILE)
  : null;

const secrets = [providerKey, password].filter(Boolean);
const stamp = new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14);
const state = {
  token: undefined,
  project: undefined,
  workspace: undefined,
  task: undefined,
  run: undefined,
  turnId: undefined,
};
const results = [];
const approvedEnterpriseMcpRequests = new Set();

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
  if (state.token) headers.set("authorization", `Bearer ${state.token}`);
  if (options.body !== undefined) headers.set("content-type", "application/json");
  const response = await fetch(`${apiBase}${pathname}`, {
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

function lastFinalReport(events) {
  return events
    .filter(
      (event) =>
        event.thread_id === state.run.codex_thread_id &&
        event.turn_id === state.turnId &&
        event.event_type === "codex.item.completed" &&
        itemType(event) === "agentMessage",
    )
    .map((event) => event.payload?.data?.text)
    .filter((text) => typeof text === "string" && text.trim())
    .at(-1);
}

function hasSection(report, chinese, english) {
  return new RegExp(`(^|\\n)#{0,4}\\s*(?:${chinese}|${english})`, "im").test(report);
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

await runCase("published enterprise governance contracts", async () => {
  const policies = await api("/supervisor-policies");
  assert(
    policies.some(
      (entry) =>
        entry.policy_id === policy.policy_id && entry.version === policy.version,
    ),
  );
  const definitions = await api("/agent-definitions");
  assert.deepEqual(
    definitions.map((entry) => entry.runtime_role).sort(),
    ["data_agent", "network_planning_agent"],
  );
  return `${policy.policy_id}@${policy.version}`;
});

await runCase("managed Workspace and Policy-bound root Thread", async () => {
  state.project = await api("/projects/managed", {
    method: "POST",
    body: { name: `East China Warehouse ${stamp}` },
  });
  state.workspace = await api("/workspaces", {
    method: "POST",
    body: {
      project_id: state.project.id,
      idempotency_key: `enterprise-workspace-${crypto.randomUUID()}`,
      kind: "main",
      name: state.project.name,
      source_ref: null,
      parent_workspace_id: null,
      copy_agents_md: false,
    },
  });
  state.task = await api("/tasks", {
    method: "POST",
    body: {
      project_id: state.project.id,
      title: "华东新增仓决策",
      model_provider: providerId,
      model,
    },
  });
  const started = await api(`/tasks/${state.task.id}/runs`, {
    method: "POST",
    body: {
      idempotency_key: `enterprise-run-${crypto.randomUUID()}`,
      workspace_id: state.workspace.id,
      fork_thread_id: null,
      fork_source_run_id: null,
      supervisor_policy: policy,
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
  assert.equal(binding.policy_id, policy.policy_id);
  assert.equal(binding.version, policy.version);

  const mcp = await eventually(async () => {
    const projection = await api(`/profile/mcp-servers?runId=${state.run.id}`);
    const encoded = JSON.stringify(projection);
    return encoded.includes("supply_chain_data") &&
      encoded.includes("supply_chain_planner")
      ? projection
      : undefined;
  }, "supply-chain MCP discovery", 120_000, 1_000);
  assert(mcp);
  return `run=${state.run.id}; root Thread=${state.run.codex_thread_id}`;
});

await runCase("real two-Agent warehouse-network collaboration", async () => {
  const prompt = `完成“华东新增仓”企业决策案例。必须遵循已绑定的 Supervisor Policy，并使用 Codex 原生多 Agent 协作：

1. 先且只创建一个 data_agent。要求它使用只读 source_id=warehouse-network-fixture，检查、构建并验证 planning-dataset.v1，然后返回未经改写的 data_ref 和 Resource name。等待它完成。
2. 取得该 data_ref 后，再且只创建一个 network_planning_agent。把未经改写的 data_ref 交给它；它必须先调用 read_mcp_resource 读取同一份 Artifact，再做任何网络计算。
3. 本案例的场景叠加已审定为 planner MCP 的 examples/network-input.json；其中既有网络必须与 planning-dataset.v1 的 network_input 完全一致，杭州与无锡是两个有限候选。若不一致必须停止。
4. 注册以下已审定导航 RouteEntry，provider=${routeFixture.provider}，method=${routeFixture.method}，require_complete=true：
${JSON.stringify(routeFixture.entries)}
5. Network Agent 必须计算并验证：实际当前覆盖、同一既有网络的优化基线、增加杭州、增加无锡；分别用 compare_network_scenarios 做同口径比较。不得用模型心算替代 MCP。
6. 你作为根 Supervisor 负责综合冲突和最终判断。不要让子 Agent 再创建 Agent，不要静默替换角色，不要请求额外业务输入。

最终报告使用清晰的“事实、假设、分析、建议、风险、缺失证据”六个部分。每个关键数字必须同时引用 Artifact schema 与工具结构化结果中的 resource_name；禁止把 Resource URI 或 data_ref.uri 当作 Resource name。说明实际当前口径和优化基线口径的区别，不能把重分配收益误算成新增仓收益。`;

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

let finalEvidence;
await runCase("Runtime Agent tree, cross-child handoff, and deterministic tools", async () => {
  const agents = await eventually(async () => {
    const current = await api(`/runs/${state.run.id}/agents`);
    const roleCounts = current.reduce((counts, agent) => {
      if (agent.agent_role) {
        counts.set(agent.agent_role, (counts.get(agent.agent_role) ?? 0) + 1);
      }
      return counts;
    }, new Map());
    return current.length === 3 &&
      roleCounts.size === 2 &&
      roleCounts.get("data_agent") === 1 &&
      roleCounts.get("network_planning_agent") === 1
      ? current
      : undefined;
  }, "root plus two Runtime child Threads");
  const root = agents.find((agent) => agent.is_root);
  const dataAgents = agents.filter((agent) => agent.agent_role === "data_agent");
  const networkAgents = agents.filter(
    (agent) => agent.agent_role === "network_planning_agent",
  );
  const [dataAgent] = dataAgents;
  const [networkAgent] = networkAgents;
  assert(root && dataAgent && networkAgent);
  assert.equal(root.agent_role, null);
  assert.equal(dataAgents.length, 1, "Automatic projection created duplicate data Agents");
  assert.equal(
    networkAgents.length,
    1,
    "Automatic projection created duplicate Network Planning Agents",
  );
  assert.equal(root.thread_id, state.run.codex_thread_id);
  assert.equal(dataAgent.parent_thread_id, root.thread_id);
  assert.equal(networkAgent.parent_thread_id, root.thread_id);
  assert.notEqual(dataAgent.thread_id, networkAgent.thread_id);
  assert(!["failed", "interrupted"].includes(dataAgent.status_type));
  assert(!["failed", "interrupted"].includes(networkAgent.status_type));

  const events = await allTaskEvents(state.task.id);
  const calls = completedMcpCalls(events);
  const dataCalls = calls.filter((event) => event.thread_id === dataAgent.thread_id);
  const networkCalls = calls.filter(
    (event) => event.thread_id === networkAgent.thread_id,
  );
  for (const tool of [
    "inspect_planning_source",
    "build_planning_dataset",
    "validate_planning_dataset",
  ]) {
    assert(
      dataCalls.some(
        (event) =>
          event.payload?.data?.server === "supply_chain_data" &&
          event.payload?.data?.tool === tool,
      ),
      `Data Agent did not complete ${tool}`,
    );
  }
  const resourceRead = networkCalls.find(
    (event) =>
      event.payload?.data?.server === "supply_chain_data" &&
      event.payload?.data?.tool === "read_mcp_resource",
  );
  const snapshotCall = networkCalls.find(
    (event) =>
      event.payload?.data?.server === "supply_chain_planner" &&
      event.payload?.data?.tool === "prepare_network_snapshot",
  );
  assert(resourceRead, "Network Agent did not read the Data Agent Resource");
  assert(snapshotCall, "Network Agent did not prepare a network snapshot");
  assert(
    resourceRead.sequence < snapshotCall.sequence,
    "Network Agent calculated before reading the Data Agent Resource",
  );
  for (const tool of [
    "register_route_matrix",
    "evaluate_current_coverage",
    "evaluate_network_scenario",
    "compare_network_scenarios",
    "validate_network_resource",
  ]) {
    assert(
      networkCalls.some(
        (event) =>
          event.payload?.data?.server === "supply_chain_planner" &&
          event.payload?.data?.tool === tool,
      ),
      `Network Agent did not complete ${tool}`,
    );
  }
  assert(
    networkCalls.filter(
      (event) => event.payload?.data?.tool === "evaluate_network_scenario",
    ).length >= 2,
    "Network Agent did not evaluate both add-warehouse candidates",
  );
  assert(
    networkCalls.filter(
      (event) => event.payload?.data?.tool === "compare_network_scenarios",
    ).length >= 2,
    "Network Agent did not compare both candidates",
  );

  finalEvidence = { agents, events, calls, dataAgent, networkAgent };
  return `root + ${agents.length - 1} child Threads; ${calls.length} MCP calls`;
});

await runCase("durable typed Artifacts and browser-safe trace", async () => {
  const requiredSchemaCounts = new Map([
    ["planning-dataset.v1", 1],
    ["network_snapshot.v1", 1],
    ["route_matrix.v1", 1],
    ["current_coverage_result.v1", 1],
    ["network_scenario_result.v1", 2],
    ["scenario_comparison.v1", 2],
  ]);
  const artifacts = await eventually(async () => {
    const current = await api(`/tasks/${state.task.id}/artifacts`);
    const failed = current.find((artifact) => artifact.state === "failed");
    if (failed) {
      throw new Error(`Artifact ${failed.id} failed materialization`);
    }
    const schemaCounts = current.reduce((counts, artifact) => {
      counts.set(
        artifact.artifact_schema,
        (counts.get(artifact.artifact_schema) ?? 0) + 1,
      );
      return counts;
    }, new Map());
    return [...requiredSchemaCounts].every(
      ([schema, count]) => (schemaCounts.get(schema) ?? 0) >= count,
    ) &&
      current.every((artifact) => artifact.state === "ready")
      ? current
      : undefined;
  }, "durable enterprise Artifacts", 180_000, 1_000);

  const planning = artifacts.find(
    (artifact) => artifact.artifact_schema === "planning-dataset.v1",
  );
  assert.equal(planning.producer_agent_role, "data_agent");
  for (const artifact of artifacts.filter(
    (entry) => entry.artifact_schema !== "planning-dataset.v1",
  )) {
    assert.equal(artifact.producer_agent_role, "network_planning_agent");
  }

  const contents = {};
  for (const artifact of artifacts) {
    const summary = await api(`/artifacts/${artifact.id}`);
    assert.equal(summary.id, artifact.id);
    assert(!Object.hasOwn(summary, "source_server"));
    assert(!Object.hasOwn(summary, "source_uri"));
    const content = await api(`/artifacts/${artifact.id}/content`);
    assert.equal(content.schema_version, artifact.artifact_schema);
    contents[artifact.id] = content;
  }

  const binding = await api(`/runs/${state.run.id}/supervisor-policy`);
  const safeTrace = JSON.stringify({
    binding,
    agents: finalEvidence.agents,
    artifacts,
    events: finalEvidence.events,
  });
  assert(!safeTrace.includes("supply-chain-data://"));
  assert(!safeTrace.includes("supply-chain://"));
  assert(!safeTrace.includes(repoRoot));

  finalEvidence.artifacts = artifacts;
  finalEvidence.contents = contents;
  finalEvidence.binding = binding;
  return `${artifacts.length} ready Artifacts; no internal Resource URI in browser DTOs`;
});

await runCase("evidence-backed Supervisor report", async () => {
  const report = lastFinalReport(finalEvidence.events);
  assert(report, "Root Supervisor did not produce a final report");
  for (const [chinese, english] of [
    ["事实", "Facts?"],
    ["假设", "Assumptions?"],
    ["分析", "Analysis"],
    ["建议", "Recommendations?"],
    ["风险", "Risks?"],
    ["缺失证据", "Missing Evidence"],
  ]) {
    assert(hasSection(report, chinese, english), `Final report omitted ${chinese}`);
  }
  for (const schema of [
    "planning-dataset.v1",
    "network_snapshot.v1",
    "route_matrix.v1",
    "scenario_comparison.v1",
  ]) {
    assert(report.includes(schema), `Final report did not cite ${schema}`);
  }
  assert(/%/.test(report), "Final report omitted coverage percentages");
  assert(/CNY/i.test(report), "Final report omitted the cost currency");
  assert(
    /实际当前|actual current/i.test(report) &&
      /优化基线|optimized existing|existing-footprint/i.test(report),
    "Final report did not distinguish actual current coverage from optimized baseline",
  );
  assert(
    /scenario_comparison\.v1[^\n]*scenario_comparison\.v1-[a-f0-9]{24}/i.test(
      report,
    ),
    "Final report did not cite a comparison Resource name",
  );
  assert(
    /network_scenario_result\.v1[^\n]*network_scenario_result\.v1-[a-f0-9]{24}/i.test(
      report,
    ),
    "Final report did not cite a scenario-result Resource name",
  );
  assert(!report.includes("[internal-resource-uri]"));
  assert(!report.includes("supply-chain-data://"));
  assert(!report.includes("supply-chain://"));
  finalEvidence.report = report;
  return `${report.length} characters with six decision sections`;
});

await runCase("browser history and evidence overview recovery", async () => {
  const turns = await api(`/runs/${state.run.id}/thread/turns`);
  const restoredReports = turns
    .flatMap((turn) => turn.items ?? [])
    .filter((item) => item.type === "agentMessage" && typeof item.text === "string")
    .map((item) => item.text)
    .filter((text) =>
      ["事实", "假设", "分析", "建议", "风险", "缺失证据"].every((section) =>
        hasSection(text, section, section),
      ),
    );
  assert(restoredReports.length > 0, "Browser history did not restore the final report");
  assert.equal(restoredReports.at(-1), finalEvidence.report);

  const [policy, agents, artifacts] = await Promise.all([
    api(`/runs/${state.run.id}/supervisor-policy`),
    api(`/runs/${state.run.id}/agents`),
    api(`/tasks/${state.task.id}/artifacts`),
  ]);
  const overview = { policy, agents, artifacts };
  assert.equal(policy.version, "1.0.0");
  assert.equal(agents.length, 3);
  assert.equal(artifacts.length, finalEvidence.artifacts.length);
  assert(artifacts.every((artifact) => artifact.state === "ready"));
  assert(!JSON.stringify(overview).includes("supply-chain://"));
  assert(!JSON.stringify(overview).includes("supply-chain-data://"));

  return `${turns.length} restored Turn; ${agents.length} Agents; ${artifacts.length} Artifacts`;
});

if (evidenceFile) {
  const evidence = {
    schemaVersion: "enterprise-supervisor-e2e.v1",
    verifiedAt: new Date().toISOString(),
    case: "华东新增仓",
    provider: { id: providerId, model },
    policy: finalEvidence.binding,
    run: {
      id: state.run.id,
      rootThreadId: state.run.codex_thread_id,
      turnId: state.turnId,
      workspaceId: state.workspace.id,
    },
    agents: finalEvidence.agents,
    toolCalls: finalEvidence.calls.map((event) => ({
      sequence: event.sequence,
      threadId: event.thread_id,
      server: event.payload?.data?.server,
      tool: event.payload?.data?.tool,
    })),
    artifacts: finalEvidence.artifacts,
    report: finalEvidence.report,
  };
  await mkdir(path.dirname(evidenceFile), { recursive: true });
  await writeFile(evidenceFile, `${JSON.stringify(evidence, null, 2)}\n`, {
    mode: 0o600,
  });
  log(`\nEvidence: ${evidenceFile}`);
}

log("\nEnterprise Supervisor E2E summary");
for (const result of results) {
  log(`- ${result.status.toUpperCase()} ${result.name} (${result.durationMs} ms)`);
}
log(`\n${results.length}/${results.length} cases passed.`);
