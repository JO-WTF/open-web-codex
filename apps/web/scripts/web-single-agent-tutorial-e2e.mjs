#!/usr/bin/env node

import assert from "node:assert/strict";
import { Blob } from "node:buffer";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const fixtureRoot = path.join(
  repoRoot,
  "docs/tutorials/assets/delivery-audit",
);
const baseUrl = (process.env.E2E_BASE_URL ?? "http://127.0.0.1:4810").replace(
  /\/$/,
  "",
);
const apiBase = `${baseUrl}/api`;
const providerId =
  process.env.E2E_PROVIDER_ID ?? "deepseek-single-agent-e2e";
const providerBaseUrl =
  process.env.E2E_PROVIDER_BASE_URL ?? "https://api.deepseek.com";
const providerWireApi = process.env.E2E_PROVIDER_WIRE_API ?? "chat";
const model = process.env.E2E_MODEL ?? "deepseek-v4-flash";
const effort = process.env.E2E_EFFORT ?? "none";
const useBuiltInProvider = process.env.E2E_USE_BUILT_IN_PROVIDER === "1";
const username = process.env.E2E_ADMIN_USERNAME ?? "single-agent-e2e";
const email =
  process.env.E2E_ADMIN_EMAIL ?? "single-agent-e2e@open-web-codex.local";
const password =
  process.env.E2E_ADMIN_PASSWORD ?? "open-web-codex-single-agent-e2e";
const providerKey = useBuiltInProvider
  ? null
  : await loadSecret(
      "DEEPSEEK_API_KEY",
      process.env.DEEPSEEK_API_KEY_FILE,
    );
const evidenceFile = process.env.E2E_EVIDENCE_FILE
  ? path.resolve(process.env.E2E_EVIDENCE_FILE)
  : null;
const secrets = [providerKey, password].filter(Boolean);
const stamp = new Date()
  .toISOString()
  .replace(/[-:.TZ]/g, "")
  .slice(0, 14);
const agentId = `tutorial-delivery-auditor-${stamp}`;
const state = {
  token: undefined,
  project: undefined,
  workspace: undefined,
  dataset: undefined,
  capability: undefined,
  agent: undefined,
  task: undefined,
  run: undefined,
  turnId: undefined,
};
const results = [];
const approvedRequests = new Set();

class ApiError extends Error {
  constructor(method, pathname, status, body) {
    super(
      `${method} ${pathname} failed (${status}): ${sanitize(body)}`,
    );
    this.status = status;
    this.body = body;
  }
}

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
  const method = options.method ?? "GET";
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
    throw new ApiError(method, pathname, response.status, body);
  }
  return body;
}

async function eventually(
  probe,
  description,
  timeoutMs = 120_000,
  intervalMs = 500,
) {
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
    `${description} timed out${
      lastError ? `: ${sanitize(lastError.message)}` : ""
    }`,
  );
}

async function runCase(name, test) {
  const started = performance.now();
  log(`\n[RUN] ${name}`);
  try {
    const details = await test();
    const durationMs = Math.round(performance.now() - started);
    results.push({ name, status: "passed", durationMs, details });
    log(
      `[PASS] ${name} (${durationMs} ms)${
        details ? ` - ${details}` : ""
      }`,
    );
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

async function allTaskEvents(taskId) {
  const events = [];
  let afterSequence = 0;
  for (;;) {
    const query = new URLSearchParams({
      after_sequence: String(afterSequence),
      limit: "200",
    });
    const page = await api(`/tasks/${taskId}/events?${query}`);
    events.push(...page);
    if (page.length < 200) return events;
    afterSequence = page.at(-1).sequence;
  }
}

function itemType(event) {
  return event.payload?.itemType ?? event.payload?.data?.type;
}

function completedToolCalls(events) {
  return events.filter(
    (event) =>
      event.event_type === "codex.item.completed" &&
      itemType(event) === "mcpToolCall",
  );
}

async function approveDeliveryTool() {
  const approvals = await api("/approvals");
  for (const approval of approvals) {
    if (
      approval.runId !== state.run.id ||
      approval.requestType !== "mcpServer/elicitation/request" ||
      approvedRequests.has(approval.id)
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
    assert.equal(requestParams?.serverName, "delivery_audit");
    const description = requestParams?.message ?? "";
    assert.match(
      description,
      /delivery[_ -]?audit|audit[_ -]?delivery[_ -]?commitments/i,
      `Unexpected MCP approval: ${description}`,
    );
    await api(`/approvals/${approval.id}/decision`, {
      method: "POST",
      body: { decision: "accept", version: approval.version },
    });
    approvedRequests.add(approval.id);
  }
}

async function waitForTurn() {
  return eventually(
    async () => {
      await approveDeliveryTool();
      const events = await allTaskEvents(state.task.id);
      const failure = events.find(
        (event) =>
          event.thread_id === state.run.codex_thread_id &&
          event.turn_id === state.turnId &&
          (event.event_type === "codex.thread.failed" ||
            event.payload?.data?.failureReason),
      );
      if (failure) {
        throw new Error(`Root Agent failed: ${sanitize(failure.payload)}`);
      }
      return events.some(
        (event) =>
          event.thread_id === state.run.codex_thread_id &&
          event.turn_id === state.turnId &&
          event.event_type === "codex.turn.completed",
      )
        ? events
        : undefined;
    },
    `Delivery audit Turn ${state.turnId}`,
    Number(process.env.E2E_TURN_TIMEOUT_MS ?? 600_000),
    1_000,
  );
}

function finalMessage(events) {
  return events
    .filter(
      (event) =>
        event.thread_id === state.run.codex_thread_id &&
        event.turn_id === state.turnId &&
        event.event_type === "codex.item.completed" &&
        itemType(event) === "agentMessage" &&
        typeof event.payload?.data?.text === "string",
    )
    .map((event) => event.payload.data.text)
    .filter((text) => text.trim())
    .at(-1);
}

function currentProviderId(catalog) {
  return catalog.currentProviderId ?? catalog.current_provider_id;
}

await runCase("authenticated single-Profile bootstrap", async () => {
  const health = await api("/health");
  assert.equal(health.ok, true);
  let auth;
  try {
    auth = await api("/sessions/local", { method: "POST" });
  } catch (error) {
    if (!(error instanceof ApiError) || error.status !== 503) throw error;
    auth = await api("/bootstrap", {
      method: "POST",
      body: {
        name: "Single Agent E2E Owner",
        username,
        email,
        password,
      },
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
    catalog = await api(`/providers/${providerId}/select`, {
      method: "POST",
    });
  } else {
    catalog = await api(`/providers/${providerId}`, {
      method: "PUT",
      body: {
        name: "Single Agent E2E Provider",
        baseUrl: providerBaseUrl,
        wireApi: providerWireApi,
        credentials: { mode: "direct", apiKey: providerKey },
        select: true,
      },
    });
    assert(
      !JSON.stringify(catalog).includes(providerKey),
      "Provider response leaked its API key",
    );
    catalog = await api(`/providers/${providerId}/models/refresh`, {
      method: "POST",
    });
  }
  assert.equal(currentProviderId(catalog), providerId);
  const provider = catalog.data.find((entry) => entry.id === providerId);
  assert(provider, `${providerId} Provider was not discovered`);
  if (!useBuiltInProvider || provider.models.length > 0) {
    assert(
      provider.models.some((entry) => entry.modelId === model),
      `${model} was not discovered`,
    );
  }
  return `provider=${providerId}; model=${model}`;
});

await runCase("managed Workspace and immutable Dataset Release", async () => {
  state.project = await api("/projects/managed", {
    method: "POST",
    body: { name: `Delivery Audit Tutorial ${stamp}` },
  });
  state.workspace = await api("/workspaces", {
    method: "POST",
    body: {
      project_id: state.project.id,
      idempotency_key: `delivery-workspace-${crypto.randomUUID()}`,
      kind: "main",
      name: state.project.name,
      source_ref: null,
      parent_workspace_id: null,
      copy_agents_md: false,
    },
  });

  const fileContract = [
    ["dataset-manifest.json", "dataset_manifest", "application/json"],
    ["deliveries.csv", "delivery_records", "text/csv"],
  ];
  const form = new FormData();
  form.append(
    "metadata",
    JSON.stringify({
      idempotency_key: `delivery-dataset-${crypto.randomUUID()}`,
      dataset_id: "delivery-commitment-audit",
      version: "1.0.0",
      display_name: "Delivery commitment audit",
      description:
        "Synthetic shipment records for the first Web-authored Agent tutorial.",
      files: fileContract.map(([logicalName, role, mediaType], index) => ({
        field_id: `file-${index + 1}`,
        logical_name: logicalName,
        role,
        media_type: mediaType,
      })),
    }),
  );
  for (const [index, [logicalName, , mediaType]] of fileContract.entries()) {
    const bytes = await readFile(path.join(fixtureRoot, logicalName));
    form.append(
      `file-${index + 1}`,
      new Blob([bytes], { type: mediaType }),
      logicalName,
    );
  }
  state.dataset = await api(
    `/workspaces/${state.workspace.id}/datasets`,
    { method: "POST", body: form },
  );
  assert.equal(state.dataset.state, "published");
  assert.equal(state.dataset.workspace_id, state.workspace.id);
  assert.equal(state.dataset.files.length, 2);
  assert.match(state.dataset.content_sha256, /^[0-9a-f]{64}$/);
  return `${state.dataset.dataset_id}@${state.dataset.version}`;
});

await runCase("validated, tested, and published Python capability", async () => {
  const [toolsSource, pythonSource, skillInstructions] = await Promise.all([
    readFile(path.join(fixtureRoot, "tools.json"), "utf8"),
    readFile(path.join(fixtureRoot, "implementation.py"), "utf8"),
    readFile(path.join(fixtureRoot, "skill-instructions.txt"), "utf8"),
  ]);
  const request = {
    idempotency_key: `delivery-capability-${crypto.randomUUID()}`,
    slug: "delivery-audit",
    version: "1.0.0",
    display_name: "Delivery audit",
    description:
      "Validate one exact delivery release and calculate commitment performance.",
    server_name: "delivery_audit",
    python_source: pythonSource,
    tools: JSON.parse(toolsSource),
    skill: {
      name: "delivery-commitment-audit",
      description:
        "Use when a user asks to audit the delivery commitment tutorial data.",
      instructions: skillInstructions,
    },
    input_artifact_types: [],
    output_artifact_types: ["delivery_audit_report.v1"],
  };
  const validation = await api(
    `/workspaces/${state.workspace.id}/python-capabilities/validate`,
    { method: "POST", body: request },
  );
  assert.equal(validation.valid, true, JSON.stringify(validation.issues));
  assert.deepEqual(validation.tool_names, ["audit_delivery_commitments"]);

  const argumentsValue = {
    workspace_id: state.workspace.id,
    release_id: state.dataset.id,
    dataset_id: state.dataset.dataset_id,
    version: state.dataset.version,
    content_sha256: state.dataset.content_sha256,
  };
  const tested = await api(
    `/workspaces/${state.workspace.id}/python-capabilities/test`,
    {
      method: "POST",
      body: {
        capability: request,
        tool_name: "audit_delivery_commitments",
        arguments: argumentsValue,
      },
    },
  );
  assert.equal(tested.result.schema_version, "delivery_audit_report.v1");
  assert.equal(tested.result.totals.shipments, 18);
  assert.equal(tested.result.totals.on_time_shipments, 11);
  assert.equal(tested.result.totals.late_shipments, 7);
  assert.equal(tested.result.totals.on_time_rate, 0.611111);
  assert.equal(
    tested.result.totals.demand_weighted_on_time_rate,
    0.674863,
  );
  assert.equal(tested.result.worst_province, "Central Java");
  assert.match(
    tested.result.resource_name,
    /^delivery_audit_report\.v1-[0-9a-f]{24}$/,
  );
  assert.equal(
    tested.result.data_ref.resource_schema,
    "delivery_audit_report.v1",
  );

  await assert.rejects(
    api(`/workspaces/${state.workspace.id}/python-capabilities/test`, {
      method: "POST",
      body: {
        capability: request,
        tool_name: "audit_delivery_commitments",
        arguments: {
          ...argumentsValue,
          workspace_id: crypto.randomUUID(),
        },
      },
    }),
    (error) =>
      error instanceof ApiError &&
      error.status === 400 &&
      /Workspace identity does not match/.test(error.message),
  );

  state.capability = await api(
    `/workspaces/${state.workspace.id}/python-capabilities/publish`,
    { method: "POST", body: request },
  );
  assert.equal(state.capability.package_id, "delivery-audit");
  assert.equal(state.capability.server_name, "delivery_audit");
  assert.match(state.capability.content_sha256, /^[0-9a-f]{64}$/);
  return `${state.capability.package_id}@${state.capability.version}`;
});

await runCase("Web-published direct root Agent Release", async () => {
  const draft = {
    definition_id: agentId,
    version: "1.0.0",
    display_name: "Tutorial Delivery Auditor",
    description:
      "Audits one exact delivery Dataset Release with the reviewed delivery Tool.",
    responsibilities: [
      "Validate one exact authorized delivery Dataset Release.",
      "Use the deterministic Tool for shipment and demand-weighted rates.",
      "Report late shipment IDs, province differences, provenance, and checks.",
    ],
    developer_instructions: `Use $delivery-commitment-audit and call delivery_audit.audit_delivery_commitments exactly once with the exact Dataset Release identity appended by the platform.
Do not scan the Workspace, open CSV files, run shell commands, or recalculate a different on-time definition.
If the Tool fails or an identity is missing, stop and report the failure.
In the final answer, cite the exact delivery_audit_report.v1 resource_name, then report shipment count, on-time and late counts, shipment on-time rate, demand-weighted on-time rate, late shipment IDs, worst province, Dataset version, and every Tool check. Do not expose the data_ref URI.`,
    input_artifact_types: [],
    output_artifact_types: ["delivery_audit_report.v1"],
    capability_template: {
      source: "workspace_package_release",
      definition_id: state.capability.package_id,
      version: state.capability.version,
      release_id: state.capability.release_id,
    },
    dataset_release_ids: [state.dataset.id],
  };
  const resource = await api("/agent-definition-resources", {
    method: "POST",
    body: draft,
  });
  const validation = await api(
    `/agent-definition-resources/${resource.id}/validate`,
    { method: "POST" },
  );
  assert.equal(validation.valid, true, JSON.stringify(validation.issues));
  assert.match(validation.content_sha256, /^[0-9a-f]{64}$/);
  assert.match(validation.execution_semantics_sha256, /^[0-9a-f]{64}$/);
  const release = await api(
    `/agent-definition-resources/${resource.id}/publish`,
    { method: "POST" },
  );
  state.agent = { draft, resource, release };

  const definitions = await api("/agent-definitions");
  const published = definitions.find(
    (entry) =>
      entry.definition_id === agentId &&
      entry.version === "1.0.0" &&
      entry.release_id === release.id,
  );
  assert(published, "Published delivery Agent was not discoverable");
  assert.equal(published.required_workspace_id, state.workspace.id);
  assert.deepEqual(
    published.dataset_releases.map((entry) => entry.release_id),
    [state.dataset.id],
  );
  assert.equal(
    published.capability_template.release_id,
    state.capability.release_id,
  );
  return `${agentId}@1.0.0`;
});

await runCase("Agent-bound root Thread", async () => {
  state.task = await api("/tasks", {
    method: "POST",
    body: {
      project_id: state.project.id,
      title: "配送承诺审计",
      model_provider: providerId,
      model,
    },
  });
  const started = await api(`/tasks/${state.task.id}/runs`, {
    method: "POST",
    body: {
      idempotency_key: `delivery-run-${crypto.randomUUID()}`,
      workspace_id: state.workspace.id,
      fork_thread_id: null,
      fork_source_run_id: null,
      supervisor_policy: null,
      agent: {
        definition_id: agentId,
        version: "1.0.0",
        release_id: state.agent.release.id,
      },
    },
  });
  state.run = await eventually(async () => {
    const run = await api(`/runs/${started.run.id}`);
    return run.codex_thread_id ? run : undefined;
  }, "Agent-bound root Thread");
  const agents = await eventually(async () => {
    const current = await api(`/runs/${state.run.id}/agents`);
    return current.length === 1 ? current : undefined;
  }, "direct root Agent projection");
  assert.equal(agents[0].is_root, true);
  assert.equal(agents[0].thread_id, state.run.codex_thread_id);
  return `run=${state.run.id}`;
});

await runCase("real Runtime Tool approval and delivery audit", async () => {
  const sent = await api(`/tasks/${state.task.id}/messages`, {
    method: "POST",
    body: {
      text: `审计已授权的配送承诺数据。告诉我配送单按时率、需求加权按时率、迟到单号和表现最差的省份，并说明使用的数据版本、Artifact resource_name 和完成了哪些校验。`,
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
  const events = await waitForTurn();
  assert.equal(approvedRequests.size, 1, "The Python Tool approval was not observed");

  const calls = completedToolCalls(events);
  assert.equal(calls.length, 1, "The Agent did not use exactly one MCP Tool");
  const call = calls[0];
  assert.equal(call.thread_id, state.run.codex_thread_id);
  assert.equal(call.payload?.data?.server, "delivery_audit");
  assert.equal(
    call.payload?.data?.tool,
    "audit_delivery_commitments",
  );
  assert.deepEqual(call.payload?.data?.arguments, {
    workspace_id: state.workspace.id,
    release_id: state.dataset.id,
    dataset_id: state.dataset.dataset_id,
    version: state.dataset.version,
    content_sha256: state.dataset.content_sha256,
  });
  const structured = call.payload?.data?.result?.structuredContent;
  assert.equal(structured.schema_version, "delivery_audit_report.v1");
  assert.equal(structured.totals.shipments, 18);
  assert.equal(structured.totals.on_time_shipments, 11);
  assert.equal(structured.totals.late_shipments, 7);
  assert.equal(structured.totals.on_time_rate, 0.611111);
  assert.equal(structured.totals.demand_weighted_on_time_rate, 0.674863);
  assert.equal(structured.worst_province, "Central Java");
  assert(!Object.hasOwn(structured, "data_ref"));

  assert.equal(
    events.filter((event) => itemType(event) === "commandExecution").length,
    0,
    "The Agent bypassed the reviewed Tool with a shell command",
  );
  assert(
    events.some(
      (event) =>
        event.event_type === "platform.approval.resolved" &&
        event.payload?.data?.approvalStatus === "accepted",
    ),
    "The accepted Tool approval was not persisted",
  );
  state.events = events;
  state.report = finalMessage(events);
  assert(state.report, "The Agent did not return a final answer");
  return `${events.length} durable events`;
});

await runCase("ready report Artifact and evidence-backed final answer", async () => {
  const artifacts = await eventually(async () => {
    const current = await api(`/tasks/${state.task.id}/artifacts`);
    const report = current.find(
      (artifact) =>
        artifact.artifact_schema === "delivery_audit_report.v1",
    );
    return report?.state === "ready" ? current : undefined;
  }, "ready delivery audit Artifact");
  assert.equal(artifacts.length, 1);
  const artifact = artifacts[0];
  assert.equal(artifact.producer_run_id, state.run.id);
  assert.equal(artifact.producer_thread_id, state.run.codex_thread_id);
  const content = await api(`/artifacts/${artifact.id}/content`);
  assert.equal(content.schema_version, "delivery_audit_report.v1");
  assert.deepEqual(content.source, {
    workspace_id: state.workspace.id,
    release_id: state.dataset.id,
    dataset_id: state.dataset.dataset_id,
    version: state.dataset.version,
    content_sha256: state.dataset.content_sha256,
    logical_file: "deliveries.csv",
    file_content_sha256: content.source.file_content_sha256,
  });
  assert.match(content.source.file_content_sha256, /^[0-9a-f]{64}$/);
  assert.deepEqual(
    content.late_shipments.map((entry) => entry.shipment_id).sort(),
    ["S002", "S005", "S006", "S008", "S011", "S012", "S015"],
  );

  const resourceName = state.events
    .find(
      (event) =>
        event.event_type === "codex.item.completed" &&
        itemType(event) === "mcpToolCall",
    )
    ?.payload?.data?.result?.structuredContent?.resource_name;
  assert.match(
    resourceName,
    /^delivery_audit_report\.v1-[0-9a-f]{24}$/,
  );
  assert(state.report.includes(resourceName));
  for (const value of ["18", "11", "7", "61.11", "67.49", "Central Java"]) {
    assert(
      state.report.includes(value),
      `Final answer omitted expected value ${value}`,
    );
  }
  for (const shipmentId of [
    "S002",
    "S005",
    "S006",
    "S008",
    "S011",
    "S012",
    "S015",
  ]) {
    assert(state.report.includes(shipmentId));
  }
  const safeProjection = JSON.stringify({
    events: state.events,
    artifacts,
    report: state.report,
  });
  assert(!safeProjection.includes("open-web-python://"));
  assert(!safeProjection.includes(repoRoot));
  state.artifacts = artifacts;
  state.artifactContent = content;
  return `${artifact.artifact_schema}; resource=${resourceName}`;
});

await runCase("browser history and durable evidence recovery", async () => {
  const turns = await api(`/runs/${state.run.id}/thread/turns`);
  const turn = turns.find((entry) => entry.id === state.turnId);
  assert(turn, "Authoritative history omitted the delivery audit Turn");
  const restoredReport = turn.items
    .filter(
      (item) =>
        item.type === "agentMessage" && typeof item.text === "string",
    )
    .map((item) => item.text)
    .filter((text) => text.trim())
    .at(-1);
  assert.equal(restoredReport, state.report);
  assert(
    turn.items.some(
      (item) =>
        item.type === "mcpToolCall" &&
        item.server === "delivery_audit" &&
        item.tool === "audit_delivery_commitments",
    ),
    "Authoritative history omitted the Tool call",
  );
  const [events, artifacts, agents] = await Promise.all([
    allTaskEvents(state.task.id),
    api(`/tasks/${state.task.id}/artifacts`),
    api(`/runs/${state.run.id}/agents`),
  ]);
  assert.equal(artifacts.length, state.artifacts.length);
  assert(artifacts.every((artifact) => artifact.state === "ready"));
  assert.equal(agents.length, 1);
  assert(
    events.some(
      (event) =>
        event.event_type === "platform.approval.resolved" &&
        event.payload?.data?.approvalStatus === "accepted",
    ),
  );
  return `${turns.length} restored Turn(s); ${artifacts.length} Artifact`;
});

if (evidenceFile) {
  const evidence = {
    schemaVersion: "web-single-agent-tutorial-evidence.v1",
    verifiedAt: new Date().toISOString(),
    provider: { id: providerId, model },
    dataset: {
      id: state.dataset.id,
      datasetId: state.dataset.dataset_id,
      version: state.dataset.version,
      contentSha256: state.dataset.content_sha256,
    },
    capability: {
      releaseId: state.capability.release_id,
      packageId: state.capability.package_id,
      version: state.capability.version,
      contentSha256: state.capability.content_sha256,
    },
    agent: {
      releaseId: state.agent.release.id,
      definitionId: agentId,
      version: "1.0.0",
    },
    run: {
      id: state.run.id,
      threadId: state.run.codex_thread_id,
      turnId: state.turnId,
      workspaceId: state.workspace.id,
    },
    artifact: state.artifacts[0],
    result: state.artifactContent,
    report: state.report,
  };
  await mkdir(path.dirname(evidenceFile), { recursive: true });
  await writeFile(
    evidenceFile,
    `${JSON.stringify(evidence, null, 2)}\n`,
    { mode: 0o600 },
  );
  log(`\nEvidence: ${evidenceFile}`);
}

log("\nWeb single-Agent tutorial E2E summary");
for (const result of results) {
  log(
    `- ${result.status.toUpperCase()} ${result.name} (${result.durationMs} ms)`,
  );
}
log(`\n${results.length}/${results.length} cases passed.`);
