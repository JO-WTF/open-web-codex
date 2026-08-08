import { existsSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { dirname, extname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = resolve(webRoot, "../..");
const fixtureRoot = resolve(
  repoRoot,
  "tools/supply-chain-network-planner/examples/indonesia-network/base",
);

const paths = {
  dataDefinition: resolve(
    repoRoot,
    "capabilities/agents/enterprise-data-agent/6.0.0/definition.json",
  ),
  dataInstructions: resolve(
    repoRoot,
    "capabilities/agents/enterprise-data-agent/6.0.0/instructions.md",
  ),
  networkDefinition: resolve(
    repoRoot,
    "capabilities/agents/enterprise-network-planning-agent/6.0.0/definition.json",
  ),
  networkInstructions: resolve(
    repoRoot,
    "capabilities/agents/enterprise-network-planning-agent/6.0.0/instructions.md",
  ),
  supervisorManifest: resolve(
    repoRoot,
    "capabilities/supervisors/enterprise-supervisor-copilot/6.0.0/manifest.json",
  ),
  artifactContracts: resolve(
    repoRoot,
    "capabilities/supervisors/enterprise-supervisor-copilot/6.0.0/artifact-contracts.json",
  ),
  supervisorInstructions: resolve(
    repoRoot,
    "capabilities/supervisors/enterprise-supervisor-copilot/6.0.0/custom-instructions.md",
  ),
  blueprintManifest: resolve(
    repoRoot,
    "capabilities/tutorial-blueprints/indonesia-warehouse-network/1.6.0/manifest.json",
  ),
  blueprintPrompt: resolve(
    repoRoot,
    "capabilities/tutorial-blueprints/indonesia-warehouse-network/1.6.0/recommended-prompt.md",
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
  if (!source) return {};
  try {
    return JSON.parse(source);
  } catch (error) {
    fail(`Invalid JSON in ${path}: ${error.message}`);
    return {};
  }
}

function requireText(source, needle, label) {
  if (!source.includes(needle)) fail(`${label} must contain ${JSON.stringify(needle)}`);
}

function forbidText(source, pattern, label) {
  if (pattern.test(source)) fail(`${label} contains prohibited text matching ${pattern}`);
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function isSha256(value) {
  return typeof value === "string" && /^[a-f0-9]{64}$/.test(value);
}

const data = readJson(paths.dataDefinition);
const network = readJson(paths.networkDefinition);
const supervisor = readJson(paths.supervisorManifest);
const contracts = readJson(paths.artifactContracts);
const blueprint = readJson(paths.blueprintManifest);
const sourceManifest = readJson(resolve(fixtureRoot, "dataset-manifest.json"));
const validation = readJson(resolve(fixtureRoot, "validation-report.json"));
const dataInstructions = read(paths.dataInstructions);
const networkInstructions = read(paths.networkInstructions);
const supervisorInstructions = read(paths.supervisorInstructions);
const blueprintPrompt = read(paths.blueprintPrompt);

const quickstart = read(resolve(paths.tutorials, "indonesia-network-quickstart.md"));
const overview = read(resolve(paths.tutorials, "supply-chain-agent-tutorial.md"));
const dataTutorial = read(resolve(paths.tutorials, "indonesia-network-01-data.md"));
const serviceTutorial = read(resolve(paths.tutorials, "indonesia-network-02-service-baseline.md"));
const currentTutorial = read(resolve(paths.tutorials, "indonesia-network-03-two-level-cost.md"));
const optimizationTutorial = read(
  resolve(paths.tutorials, "indonesia-network-04-optimization-map.md"),
);

if (data.version !== "6.0.0" || network.version !== "6.0.0") {
  fail("The current tutorial must bind Data Agent and Network Agent 6.0.0");
}
if (data.definitionId !== "enterprise-data-agent") fail("Data Agent identity drifted");
if (network.definitionId !== "enterprise-network-planning-agent") {
  fail("Network Agent identity drifted");
}
if (blueprint.blueprintId !== "indonesia-warehouse-network" || blueprint.revision !== "1.6.0") {
  fail("Tutorial Blueprint identity drifted");
}
if (supervisor.policyId !== "enterprise-supervisor-copilot" || supervisor.version !== "6.0.0") {
  fail("Supervisor package identity drifted");
}

const supervisorAgentKeys = new Set(
  (supervisor.agents ?? []).map((agent) => `${agent.definitionId}@${agent.version}`),
);
if (
  supervisorAgentKeys.size !== 2 ||
  !supervisorAgentKeys.has("enterprise-data-agent@6.0.0") ||
  !supervisorAgentKeys.has("enterprise-network-planning-agent@6.0.0")
) {
  fail("Current Supervisor must bind exactly Data Agent and Network Agent 6.0.0");
}
if ((blueprint.agentTemplates ?? []).length !== 2) {
  fail("Current Blueprint must bind exactly two domain Agents");
}

const expectedMcpServers = new Set(["supply_chain_network"]);
const declaredMcpServers = new Set(blueprint.requiredMcpServers ?? []);
if (
  declaredMcpServers.size !== expectedMcpServers.size ||
  [...expectedMcpServers].some((name) => !declaredMcpServers.has(name))
) {
  fail("Current Blueprint MCP requirements drifted");
}
const agentMcpServers = new Set(
  [data, network].flatMap((definition) =>
    (definition.requiredMcpServers ?? []).map((server) => server.name),
  ),
);
if ([...agentMcpServers].some((name) => !declaredMcpServers.has(name))) {
  fail("Blueprint MCP requirements omit an Agent capability");
}

if (
  blueprint.dataset?.datasetId !== sourceManifest.dataset_id ||
  blueprint.dataset?.version !== sourceManifest.version ||
  blueprint.dataset?.sourceContentSha256 !== sourceManifest.content_sha256
) {
  fail("Blueprint Dataset identity/hash does not match the generated fixture");
}
const sourceFiles = new Set(["dataset-manifest.json", ...(sourceManifest.files ?? []).map((file) => file.path)]);
const blueprintFiles = new Set((blueprint.dataset?.files ?? []).map((file) => file.logicalName));
if (
  sourceFiles.size !== blueprintFiles.size ||
  [...sourceFiles].some((name) => !blueprintFiles.has(name))
) {
  fail("Blueprint Dataset file list drifted from the generated fixture");
}
for (const file of blueprint.dataset?.files ?? []) {
  const bytes = readFileSync(resolve(fixtureRoot, file.logicalName));
  if (bytes.length !== file.bytes || sha256(bytes) !== file.sha256) {
    fail(`Blueprint fixture digest drifted for ${file.logicalName}`);
  }
}
for (const file of sourceManifest.files ?? []) {
  const bytes = readFileSync(resolve(fixtureRoot, file.path));
  if (bytes.length !== file.bytes || sha256(bytes) !== file.sha256) {
    fail(`Source manifest digest drifted for ${file.path}`);
  }
}

const contractTypes = (contracts.contracts ?? []).map((contract) => contract.artifactType);
const blueprintTypes = blueprint.expectedArtifactTypes ?? [];
if (
  new Set(contractTypes).size !== contractTypes.length ||
  new Set(blueprintTypes).size !== blueprintTypes.length ||
  contractTypes.length !== blueprintTypes.length ||
  contractTypes.some((type) => !blueprintTypes.includes(type))
) {
  fail("Current Supervisor and Blueprint Artifact contracts drifted");
}
for (const type of ["network_comparison_map.v1", "network_planning_report.v1"]) {
  requireText(quickstart + dataTutorial + serviceTutorial + currentTutorial + optimizationTutorial, type, "tutorial artifact contract");
}
for (const facet of [
  "requirements",
  "source_inventory",
  "mapping",
  "normalized_input",
  "route_matrix",
  "cost_matrix",
  "baseline",
  "scenario",
  "facility_location",
]) {
  requireText(
    quickstart + dataTutorial + serviceTutorial + currentTutorial + optimizationTutorial,
    facet,
    "tutorial Network Case contract",
  );
}

for (const [label, source] of [
  ["Data Agent instructions", dataInstructions],
  ["Network Agent instructions", networkInstructions],
  ["Supervisor instructions", supervisorInstructions],
  ["Blueprint prompt", blueprintPrompt],
]) {
  requireText(source, "Network Case", label);
  forbidText(
    source,
    /supply_chain_(?:data|planner|demo|indonesia)|planning-dataset\.v2|source\s+.+\.sh/i,
    label,
  );
}
requireText(supervisorInstructions, "Data Agent", "Supervisor responsibilities");
requireText(supervisorInstructions, "Network Agent", "Supervisor responsibilities");
requireText(supervisorInstructions, "request_user_input", "Supervisor input contract");
requireText(supervisorInstructions, "当前覆盖", "Supervisor current-plan label");

requireText(quickstart, "enterprise-supervisor-copilot@6.0.0", "quickstart Supervisor");
requireText(quickstart, "optimized_existing_footprint", "quickstart baseline label");
requireText(dataTutorial, "50 个印尼需求城市", "data tutorial fixture scope");
requireText(dataTutorial, "550", "data tutorial route count");
requireText(serviceTutorial, "cost_matrix", "cost tutorial matrix");
requireText(serviceTutorial, "linehaul", "cost tutorial network layer");
requireText(currentTutorial, "actual_current", "current coverage tutorial label");
requireText(currentTutorial, "current-coverage.csv", "current coverage tutorial input");
requireText(optimizationTutorial, "facility_location", "optimization solution");
requireText(optimizationTutorial, "network_comparison_map.v1", "optimization map");

if (
  validation.demand?.city_count !== 50 ||
  validation.demand?.formula_pass !== true ||
  validation.network?.existing_warehouse_count !== 11 ||
  validation.network?.center_count !== 5 ||
  validation.network?.cross_docking_count !== 6 ||
  validation.quotes?.warehouse_to_demand_row_count !== 550 ||
  validation.quotes?.linehaul_row_count !== 30 ||
  validation.quotes?.total_row_count !== 580 ||
  validation.all_passed !== true
) {
  fail("Generated Indonesia fixture quality gates drifted");
}

for (const [name, source] of [
  ["quickstart", quickstart],
  ["overview", overview],
  ["data tutorial", dataTutorial],
  ["service tutorial", serviceTutorial],
  ["current coverage tutorial", currentTutorial],
  ["optimization tutorial", optimizationTutorial],
]) {
  forbidText(source, /(?:^|[\s("'`])(?:\/Users\/|\/home\/|file:\/\/)[^\s)"'`]*/m, name);
  forbidText(source, /\b(?:mcp|resource|codex):\/\/[^\s)"'`]*/i, name);
  forbidText(source, /\bsk-[A-Za-z0-9_-]{12,}\b/, name);
  forbidText(
    source,
    /supply_chain_(?:data|planner|demo|indonesia)|planning-dataset\.v2|(?:data_requirement_profile|normalized_network_input|route_matrix|cost_matrix|network_assignment|network_service_metrics|network_cost_summary|network_scenario|facility_location_solution)\.v\d+/,
    name,
  );

  const linkPattern = /\[[^\]]+\]\(([^)]+)\)/g;
  for (const match of source.matchAll(linkPattern)) {
    const target = match[1].trim();
    if (!target || target.startsWith("#") || /^[a-z][a-z0-9+.-]*:/i.test(target)) continue;
    const withoutFragment = target.split("#", 1)[0];
    if (!withoutFragment) continue;
    const resolved = resolve(dirname(resolve(paths.tutorials, `${name}.md`)), decodeURIComponent(withoutFragment));
    if (!existsSync(resolved)) fail(`${name} links to missing local target ${target}`);
    else if (extname(resolved) === ".md" && !read(resolved).trim()) fail(`${name} links to empty Markdown target ${target}`);
  }
}

if (failures.length > 0) {
  console.error("Tutorial contract check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(
  `Tutorial contracts pass: Blueprint ${blueprint.revision}, Supervisor ${supervisor.version}, Network ${network.version}, one Network Case and ${contractTypes.length} final Artifact contracts.`,
);
