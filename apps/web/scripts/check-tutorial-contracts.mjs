import { existsSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, extname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(webRoot, "../..");

const paths = {
  networkDefinition: resolve(
    repoRoot,
    "capabilities/agents/enterprise-network-planning-agent/5.0.0/definition.json",
  ),
  networkInstructions: resolve(
    repoRoot,
    "capabilities/agents/enterprise-network-planning-agent/5.0.0/instructions.md",
  ),
  supervisorManifest: resolve(
    repoRoot,
    "capabilities/supervisors/enterprise-supervisor-copilot/5.0.0/manifest.json",
  ),
  artifactContracts: resolve(
    repoRoot,
    "capabilities/supervisors/enterprise-supervisor-copilot/5.0.0/artifact-contracts.json",
  ),
  supervisorInstructions: resolve(
    repoRoot,
    "capabilities/supervisors/enterprise-supervisor-copilot/5.0.0/custom-instructions.md",
  ),
  visualizationInstructions: resolve(
    repoRoot,
    "capabilities/agents/enterprise-visualization-agent/2.0.0/instructions.md",
  ),
  datasetManifest: resolve(
    repoRoot,
    "tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/dataset-manifest.json",
  ),
  validationReport: resolve(
    repoRoot,
    "tools/supply-chain-network-planner/examples/indonesia-tutorial/releases/1.0.0/validation-report.json",
  ),
  blueprintManifest: resolve(
    repoRoot,
    "capabilities/tutorial-blueprints/indonesia-warehouse-network/1.5.0/manifest.json",
  ),
  blueprintPrompt: resolve(
    repoRoot,
    "capabilities/tutorial-blueprints/indonesia-warehouse-network/1.5.0/recommended-prompt.md",
  ),
  tutorials: resolve(repoRoot, "docs/tutorials"),
};

const failures = [];

function fail(message) {
  failures.push(message);
}

function read(path) {
  if (!existsSync(path)) {
    fail(`Missing required file: ${path}`);
    return "";
  }
  return readFileSync(path, "utf8");
}

function readJson(path) {
  const source = read(path);
  if (!source) {
    return {};
  }
  try {
    return JSON.parse(source);
  } catch (error) {
    fail(`Invalid JSON in ${path}: ${error.message}`);
    return {};
  }
}

function requireText(source, needle, label) {
  if (!source.includes(needle)) {
    fail(`${label} must contain ${JSON.stringify(needle)}`);
  }
}

function forbidText(source, pattern, label) {
  if (pattern.test(source)) {
    fail(`${label} contains prohibited text matching ${pattern}`);
  }
}

function formatInteger(value) {
  return new Intl.NumberFormat("en-US", { maximumFractionDigits: 0 }).format(value);
}

function isSha256(value) {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
}

const network = readJson(paths.networkDefinition);
const supervisor = readJson(paths.supervisorManifest);
const contracts = readJson(paths.artifactContracts);
const dataset = readJson(paths.datasetManifest);
const validation = readJson(paths.validationReport);
const blueprint = readJson(paths.blueprintManifest);
const networkInstructions = read(paths.networkInstructions);
const supervisorInstructions = read(paths.supervisorInstructions);
const visualizationInstructions = read(paths.visualizationInstructions);

const quickstartPath = resolve(paths.tutorials, "indonesia-network-quickstart.md");
const overviewPath = resolve(paths.tutorials, "supply-chain-agent-tutorial.md");
const servicePath = resolve(paths.tutorials, "indonesia-network-02-service-baseline.md");
const costPath = resolve(paths.tutorials, "indonesia-network-03-two-level-cost.md");
const completePath = resolve(paths.tutorials, "indonesia-network-04-optimization-map.md");

const quickstart = read(quickstartPath);
const overview = read(overviewPath);
const service = read(servicePath);
const cost = read(costPath);
const complete = read(completePath);

if (network.definitionId !== "enterprise-network-planning-agent") {
  fail("Network definition identity drifted");
}
if (network.version !== "5.0.0") {
  fail(`Expected Network Agent 5.0.0, found ${network.version ?? "missing"}`);
}

if (
  blueprint.blueprintId !== "indonesia-warehouse-network" ||
  blueprint.revision !== "1.5.0"
) {
  fail("Tutorial Blueprint identity drifted");
}
for (const marker of [
  "MAP_HANDOFF",
  "map_manifest_resource_name",
  "geojson_resource_name",
  "map_artifact_id",
  "structuredContent.embed.code",
]) {
  requireText(
    visualizationInstructions,
    marker,
    "Visualization Agent stable map handoff",
  );
}
requireText(
  visualizationInstructions,
  "Do not reproduce renderer JSON or the GeoJSON body",
  "Visualization Agent non-duplicated embed contract",
);
if (
  blueprint.dataset?.datasetId !== dataset.dataset_id ||
  blueprint.dataset?.version !== dataset.version ||
  blueprint.dataset?.sourceContentSha256 !== dataset.content_sha256
) {
  fail("Tutorial Blueprint Dataset identity/hash does not match the source manifest");
}

const sourceDatasetFiles = new Set([
  "dataset-manifest.json",
  ...((dataset.files ?? []).map((file) => file.path)),
]);
const blueprintDatasetFiles = new Set(
  (blueprint.dataset?.files ?? []).map((file) => file.logicalName),
);
if (
  sourceDatasetFiles.size !== blueprintDatasetFiles.size ||
  [...sourceDatasetFiles].some((file) => !blueprintDatasetFiles.has(file))
) {
  fail("Tutorial Blueprint Dataset file list drifted from the source release");
}

const requiredMcpServers = new Set();
for (const template of blueprint.agentTemplates ?? []) {
  const definitionPath = resolve(
    repoRoot,
    "capabilities/agents",
    template.definitionId,
    template.version,
    "definition.json",
  );
  const definition = readJson(definitionPath);
  if (
    definition.definitionId !== template.definitionId ||
    definition.version !== template.version
  ) {
    fail(
      `Tutorial Blueprint Agent template is unavailable: ${template.definitionId}@${template.version}`,
    );
  }
  if (!isSha256(template.contentSha256)) {
    fail(
      `Tutorial Blueprint Agent template has no exact content hash: ${template.definitionId}@${template.version}`,
    );
  }
  for (const server of definition.requiredMcpServers ?? []) {
    requiredMcpServers.add(server.name);
  }
}
const blueprintMcpServers = new Set(blueprint.requiredMcpServers ?? []);
if (
  [...requiredMcpServers].some((server) => !blueprintMcpServers.has(server)) ||
  !blueprintMcpServers.has("supply_chain_indonesia")
) {
  fail("Tutorial Blueprint MCP requirements omit an Agent or tutorial-only server");
}

if (
  blueprint.supervisorTemplate?.policyId !== supervisor.policyId ||
  blueprint.supervisorTemplate?.version !== supervisor.version
) {
  fail("Tutorial Blueprint Supervisor template drifted from the Indonesia Network Planning Copilot Draft");
}
if (!isSha256(blueprint.supervisorTemplate?.contentSha256)) {
  fail("Tutorial Blueprint Supervisor template has no exact content hash");
}

const instructionPolicy = blueprint.instructionPolicyTemplate ?? {};
const instructionPolicyManifestPath = resolve(
  repoRoot,
  "capabilities/supervisor-instruction-policies",
  instructionPolicy.policyId ?? "",
  instructionPolicy.version ?? "",
  "manifest.json",
);
const instructionPolicyManifest = readJson(instructionPolicyManifestPath);
const instructionPolicyText = read(
  resolve(dirname(instructionPolicyManifestPath), instructionPolicyManifest.instructionsFile ?? ""),
).trim();
const instructionPolicyHash = createHash("sha256")
  .update(instructionPolicyText)
  .digest("hex");
if (
  instructionPolicyManifest.policyId !== instructionPolicy.policyId ||
  instructionPolicyManifest.version !== instructionPolicy.version ||
  instructionPolicy.contentSha256 !== instructionPolicyHash
) {
  fail("Tutorial Blueprint instruction-policy exact reference/hash drifted");
}
if (blueprint.recommendedPromptFile !== "recommended-prompt.md") {
  fail("Tutorial Blueprint recommended Prompt identity drifted");
}
if (!read(paths.blueprintPrompt).trim()) {
  fail("Tutorial Blueprint recommended Prompt is empty");
}

const supervisorNetwork = supervisor.agents?.find(
  (agent) => agent.definitionId === network.definitionId,
);
if (supervisorNetwork?.version !== network.version) {
  fail("Supervisor does not bind the current exact Network Agent version");
}

const contractTypes = contracts.contracts?.map((contract) => contract.artifactType) ?? [];
const uniqueContractTypes = new Set(contractTypes);
if (uniqueContractTypes.size !== contractTypes.length) {
  fail("Supervisor Artifact contracts contain duplicate artifact types");
}

const deliveryTypes = ["report.v1", "map.v3"];
const blueprintArtifactTypes = new Set(blueprint.expectedArtifactTypes ?? []);
const resourceTypes = [...blueprintArtifactTypes].filter(
  (type) => !deliveryTypes.includes(type),
);
if (!blueprintArtifactTypes.has("report.v1") || !blueprintArtifactTypes.has("map.v3")) {
  fail("Tutorial Blueprint must declare report.v1 and map.v3 delivery Artifacts");
}

for (const [label, source] of [
  ["quickstart", quickstart],
  ["complete tutorial", complete],
]) {
  for (const type of resourceTypes) {
    requireText(source, type, label);
  }
  for (const type of deliveryTypes) {
    requireText(source, type, label);
  }
}

for (const marker of [
  "indonesia_decision_report.v1",
  "report.v1",
  "REPORT_HANDOFF",
  "report_resource_name",
  "report_artifact_id",
]) {
  requireText(complete, marker, "complete tutorial typed report delivery");
}
requireText(
  complete,
  "模型正文不是报告来源",
  "complete tutorial report authority",
);
requireText(
  supervisorInstructions,
  "An empty Workspace is a real zero-source result",
  "Supervisor real Workspace isolation",
);
requireText(
  supervisorInstructions,
  "synthetic_demo",
  "Supervisor Demo provenance",
);

requireText(
  service,
  `Enterprise Network Planning Agent · ${network.version}`,
  "service tutorial",
);
requireText(
  cost,
  `Enterprise Network Planning Agent · ${network.version}`,
  "cost tutorial",
);
forbidText(
  `${service}\n${cost}\n${complete}`,
  /Enterprise Network Planning Agent · 3\.1\.0/,
  "network tutorials",
);

requireText(overview, "deterministic decision report", "tutorial overview");
requireText(complete, "以下八个 Resource Artifact", "complete tutorial");
requireText(
  complete,
  "prepare_indonesia_decision_report",
  "complete tutorial",
);
requireText(
  complete,
  "模型正文不是报告来源",
  "complete tutorial report ownership",
);
forbidText(
  `${quickstart}\n${complete}`,
  /\breport_markdown\b/,
  "typed report tutorials",
);
forbidText(
  `${quickstart}\n${complete}`,
  /(?:最终正文必须等于|原样\s+report_markdown|原样报告|byte[- ]for[- ]byte)/i,
  "typed report tutorial delivery",
);

requireText(
  quickstart,
  `${blueprint.blueprintId}@${blueprint.revision}`,
  "quickstart Blueprint",
);
requireText(
  quickstart,
  `${blueprint.displayName} · ${blueprint.revision}`,
  "quickstart Blueprint display",
);
for (const label of [
  "Learn",
  "Set up example",
  "Readiness",
  "Start task",
  "Send",
]) {
  requireText(quickstart, label, "quickstart UI");
}

for (const label of ["Add data", "Publish a data release"]) {
  const builderDocs = `${read(resolve(paths.tutorials, "indonesia-network-01-data.md"))}\n${read(
    resolve(paths.tutorials, "web-single-agent-delivery-audit.md"),
  )}`;
  requireText(builderDocs, label, "Builder data tutorials");
}

const expectedDatasetFacts = [
  dataset.customer_count,
  validation.demand?.annual_demand_units,
  validation.geometry?.current_province_count,
  validation.network?.central_warehouse_count,
  validation.network?.forward_warehouse_count,
  validation.network?.candidate_location_count,
  validation.quotes?.row_count,
];
const dataTutorial = read(resolve(paths.tutorials, "indonesia-network-01-data.md"));
for (const value of expectedDatasetFacts) {
  if (!Number.isFinite(value)) {
    fail("Dataset validation report is missing a required known-answer value");
    continue;
  }
  requireText(dataTutorial, formatInteger(value), "data tutorial known answers");
}

for (const value of [
  976_434_900,
  16_425_000_000,
  15_448_565_100,
]) {
  requireText(cost, formatInteger(value), "cost tutorial exact IDR");
}
for (const value of [2_835_533_800, 13_589_466_200]) {
  requireText(complete, formatInteger(value), "complete tutorial exact IDR");
  requireText(quickstart, formatInteger(value), "quickstart exact IDR");
}

forbidText(
  `${cost}\n${complete}`,
  /\b(?:million|billion)\s+IDR\b/i,
  "Indonesia monetary conclusions",
);
forbidText(
  complete,
  /(?:以下|至少应产生以下)六个 Resource Artifact/,
  "complete tutorial Resource count",
);
forbidText(
  read(resolve(paths.tutorials, "README.md")),
  /使用 `1\.0\.1`/,
  "tutorial version guidance",
);

const tutorialFiles = [
  "README.md",
  "indonesia-network-quickstart.md",
  "supply-chain-agent-tutorial.md",
  "indonesia-network-01-data.md",
  "indonesia-network-02-service-baseline.md",
  "indonesia-network-03-two-level-cost.md",
  "indonesia-network-04-optimization-map.md",
  "web-single-agent-delivery-audit.md",
  "approvals-and-recovery.md",
  "hello-agent-quickstart.md",
].map((name) => resolve(paths.tutorials, name));

for (const path of tutorialFiles) {
  const source = read(path);
  const label = path.slice(repoRoot.length + 1);
  forbidText(source, /(?:^|[\s("'`])(?:\/Users\/|\/home\/|file:\/\/)[^\s)"'`]*/m, label);
  forbidText(source, /\b(?:mcp|resource|codex):\/\/[^\s)"'`]*/i, label);
  forbidText(source, /\bsk-[A-Za-z0-9_-]{12,}\b/, label);
  forbidText(
    source,
    /\*\*(?:New thread|Start governed agent|Start governed supervisor)\*\*|机器人图标|星光图标/,
    label,
  );
  forbidText(
    source,
    /\brequest[_ -]?id\s*[:=]\s*["'`]?[A-Za-z0-9_-]{6,}/i,
    label,
  );

  const linkPattern = /\[[^\]]+\]\(([^)]+)\)/g;
  for (const match of source.matchAll(linkPattern)) {
    const target = match[1].trim();
    if (
      !target ||
      target.startsWith("#") ||
      /^[a-z][a-z0-9+.-]*:/i.test(target)
    ) {
      continue;
    }
    const withoutFragment = target.split("#", 1)[0];
    if (!withoutFragment) {
      continue;
    }
    const resolved = resolve(dirname(path), decodeURIComponent(withoutFragment));
    if (!existsSync(resolved)) {
      fail(`${label} links to missing local target ${target}`);
    } else if (extname(resolved) === ".md" && !read(resolved).trim()) {
      fail(`${label} links to empty Markdown target ${target}`);
    }
  }
}

if (failures.length > 0) {
  console.error("Tutorial contract check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(
  `Tutorial contracts pass: Blueprint ${blueprint.revision}, Supervisor ${supervisor.version}, Network ${network.version}, ${contractTypes.length} handoffs, ${resourceTypes.length} Resources + report.v1 + map.v3.`,
);
