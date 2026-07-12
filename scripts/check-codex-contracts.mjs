import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const CONTRACT_PATHS = {
  registry: "contracts/codex/capability-ids.v1.json",
  manifestSchema: "contracts/codex/capability-manifest.schema.json",
  manifestFixture: "contracts/codex/fixtures/capability-manifest.v1.json",
  errorSchema: "contracts/codex/error.schema.json",
  fixtureSchema: "contracts/codex/protocol-fixture.schema.json",
  bundleSchema: "contracts/codex/contract-bundle.schema.json",
  compatibility: "contracts/codex/compatibility-matrix.json",
};

const SEMVER = /^[0-9]+\.[0-9]+\.[0-9]+$/;
const CAPABILITY_ID = /^[a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+$/;
const CAPABILITY_STATUSES = new Set([
  "supported",
  "unsupported",
  "degraded",
  "incompatible",
  "experimental",
]);

function parseArgs(argv) {
  const options = { base: null, selfTest: false };
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--self-test") {
      options.selfTest = true;
    } else if (value === "--base") {
      options.base = argv[index + 1] ?? null;
      index += 1;
    } else {
      throw new Error(`Unknown argument: ${value}`);
    }
  }
  return options;
}

function readJson(path) {
  return JSON.parse(readFileSync(resolve(path), "utf8"));
}

function runGit(args) {
  return execFileSync("git", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trimEnd();
}

function resolveBase(requestedBase) {
  if (!requestedBase || /^0+$/.test(requestedBase)) {
    try {
      runGit(["rev-parse", "HEAD^"]);
      return "HEAD^";
    } catch {
      return null;
    }
  }
  runGit(["rev-parse", "--verify", `${requestedBase}^{commit}`]);
  return runGit(["merge-base", requestedBase, "HEAD"]);
}

function readBaseJson(base, path) {
  if (!base) {
    return null;
  }
  try {
    return JSON.parse(runGit(["show", `${base}:${path}`]));
  } catch {
    return null;
  }
}

function assert(condition, message, errors) {
  if (!condition) {
    errors.push(message);
  }
}

function uniqueStrings(values) {
  return (
    Array.isArray(values) &&
    values.every((value) => typeof value === "string" && value.length > 0) &&
    new Set(values).size === values.length
  );
}

export function validateContracts(contracts) {
  const errors = [];
  const {
    registry,
    manifestSchema,
    manifestFixture,
    errorSchema,
    fixtureSchema,
    bundleSchema,
    compatibility,
  } = contracts;

  assert(SEMVER.test(registry.schemaVersion ?? ""), "registry.schemaVersion must be SemVer", errors);
  assert(Array.isArray(registry.capabilities), "registry.capabilities must be an array", errors);

  const registryEntries = Array.isArray(registry.capabilities) ? registry.capabilities : [];
  const registryIds = registryEntries.map((entry) => entry.id);
  assert(new Set(registryIds).size === registryIds.length, "registry capability IDs must be unique", errors);
  for (const entry of registryEntries) {
    assert(CAPABILITY_ID.test(entry.id ?? ""), `invalid registry capability ID: ${entry.id}`, errors);
    assert(entry.owner === "codex-rust", `${entry.id} must be owned by codex-rust`, errors);
    assert(typeof entry.requiredForV1 === "boolean", `${entry.id} must declare requiredForV1`, errors);
    assert(typeof entry.description === "string" && entry.description.length > 0, `${entry.id} needs a description`, errors);
  }

  assert(SEMVER.test(manifestFixture.schemaVersion ?? ""), "manifest schemaVersion must be SemVer", errors);
  assert(Array.isArray(manifestFixture.capabilities), "manifest capabilities must be an array", errors);
  const fixtureCapabilities = Array.isArray(manifestFixture.capabilities)
    ? manifestFixture.capabilities
    : [];
  const fixtureIds = fixtureCapabilities.map((entry) => entry.id);
  assert(new Set(fixtureIds).size === fixtureIds.length, "manifest capability IDs must be unique", errors);
  for (const id of registryIds) {
    assert(fixtureIds.includes(id), `manifest Fixture is missing registered capability ${id}`, errors);
  }
  for (const id of fixtureIds) {
    assert(registryIds.includes(id), `manifest Fixture contains unregistered capability ${id}`, errors);
  }

  for (const capability of fixtureCapabilities) {
    assert(CAPABILITY_ID.test(capability.id ?? ""), `invalid manifest capability ID: ${capability.id}`, errors);
    assert(SEMVER.test(capability.version ?? ""), `${capability.id} version must be SemVer`, errors);
    assert(CAPABILITY_STATUSES.has(capability.status), `${capability.id} has invalid status`, errors);
    assert(capability.methods && typeof capability.methods === "object", `${capability.id} needs methods`, errors);
    assert(capability.limits && typeof capability.limits === "object", `${capability.id} needs limits`, errors);
    for (const key of ["clientRequests", "serverRequests", "notifications"]) {
      if (capability.methods?.[key] !== undefined) {
        assert(uniqueStrings(capability.methods[key]), `${capability.id}.${key} must contain unique strings`, errors);
      }
    }
    if (["unsupported", "degraded", "incompatible"].includes(capability.status)) {
      assert(
        typeof capability.reason?.code === "string" &&
          typeof capability.reason?.message === "string",
        `${capability.id} status ${capability.status} requires a structured reason`,
        errors,
      );
    }
    if (capability.status === "experimental") {
      assert(capability.experimental === true, `${capability.id} experimental status requires experimental=true`, errors);
    }
  }

  assert(manifestSchema.$defs?.capability, "manifest Schema must define capability", errors);
  assert(manifestSchema.$defs?.methodSet, "manifest Schema must define methodSet", errors);
  assert(Array.isArray(errorSchema.properties?.category?.enum), "error Schema must define categories", errors);
  assert(Array.isArray(fixtureSchema.properties?.messages?.items?.properties?.kind?.enum), "fixture Schema must define message kinds", errors);
  assert(Array.isArray(bundleSchema.properties?.files?.items?.required), "bundle Schema must define required file fields", errors);
  assert(SEMVER.test(compatibility.matrixVersion ?? ""), "compatibility matrixVersion must be SemVer", errors);
  assert(Array.isArray(compatibility.serverBuilds), "compatibility.serverBuilds must be an array", errors);
  assert(Array.isArray(compatibility.releaseOrder) && compatibility.releaseOrder.length > 0, "compatibility.releaseOrder is required", errors);

  return errors;
}

function removedValues(previous, current, label) {
  if (!Array.isArray(previous) || !Array.isArray(current)) {
    return [];
  }
  return previous.filter((value) => !current.includes(value)).map((value) => `${label} removed: ${value}`);
}

function addedRequiredFields(previous, current, label) {
  if (!Array.isArray(previous) || !Array.isArray(current)) {
    return [];
  }
  return current
    .filter((value) => !previous.includes(value))
    .map((value) => `${label} required field added in v1: ${value}`);
}

export function findBreakingChanges(current, previous) {
  const changes = [];
  if (!previous) {
    return changes;
  }

  const previousIds = new Map(previous.registry.capabilities.map((entry) => [entry.id, entry]));
  const currentIds = new Map(current.registry.capabilities.map((entry) => [entry.id, entry]));
  for (const [id, entry] of previousIds) {
    if (!currentIds.has(id)) {
      changes.push(`capability ID removed: ${id}`);
    } else if (entry.requiredForV1 === true && currentIds.get(id).requiredForV1 !== true) {
      changes.push(`required V1 capability downgraded: ${id}`);
    }
  }

  changes.push(
    ...addedRequiredFields(
      previous.manifestSchema.$defs?.capability?.required,
      current.manifestSchema.$defs?.capability?.required,
      "capability",
    ),
    ...addedRequiredFields(
      previous.manifestSchema.required,
      current.manifestSchema.required,
      "Manifest",
    ),
    ...addedRequiredFields(previous.errorSchema.required, current.errorSchema.required, "error"),
    ...addedRequiredFields(
      previous.fixtureSchema.required,
      current.fixtureSchema.required,
      "protocol Fixture",
    ),
    ...addedRequiredFields(
      previous.bundleSchema.required,
      current.bundleSchema.required,
      "contract bundle",
    ),
  );

  changes.push(
    ...removedValues(
      previous.manifestSchema.$defs?.capability?.properties?.status?.enum,
      current.manifestSchema.$defs?.capability?.properties?.status?.enum,
      "capability status",
    ),
  );
  changes.push(
    ...removedValues(
      previous.errorSchema.properties?.category?.enum,
      current.errorSchema.properties?.category?.enum,
      "error category",
    ),
  );
  changes.push(
    ...removedValues(
      previous.fixtureSchema.properties?.messages?.items?.properties?.kind?.enum,
      current.fixtureSchema.properties?.messages?.items?.properties?.kind?.enum,
      "Fixture message kind",
    ),
  );

  return changes;
}

function loadCurrentContracts() {
  return Object.fromEntries(
    Object.entries(CONTRACT_PATHS).map(([key, path]) => [key, readJson(path)]),
  );
}

function loadPreviousContracts(base) {
  const values = Object.fromEntries(
    Object.entries(CONTRACT_PATHS).map(([key, path]) => [key, readBaseJson(base, path)]),
  );
  return Object.values(values).every(Boolean) ? values : null;
}

function runSelfTest() {
  const current = loadCurrentContracts();
  const validationErrors = validateContracts(current);
  if (validationErrors.length > 0) {
    throw new Error(`Current contract validation failed:\n${validationErrors.join("\n")}`);
  }

  const previous = structuredClone(current);
  previous.registry.capabilities.push({
    id: "removed.capability",
    owner: "codex-rust",
    requiredForV1: true,
    description: "Self-test only",
  });
  previous.manifestSchema.$defs.capability.properties.status.enum.push("retired-test");
  current.manifestSchema.$defs.capability.required.push("newRequiredField");
  const changes = findBreakingChanges(current, previous);
  if (
    !changes.some((value) => value.includes("removed.capability")) ||
    !changes.some((value) => value.includes("retired-test")) ||
    !changes.some((value) => value.includes("newRequiredField"))
  ) {
    throw new Error("Codex contract breaking-change self-test failed");
  }
  process.stdout.write("Codex contract self-test passed.\n");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.selfTest) {
    runSelfTest();
    return;
  }

  const current = loadCurrentContracts();
  const validationErrors = validateContracts(current);
  const base = resolveBase(options.base);
  const previous = loadPreviousContracts(base);
  const breakingChanges = findBreakingChanges(current, previous);

  if (validationErrors.length === 0 && breakingChanges.length === 0) {
    const comparison = previous ? ` compared with ${base}` : " (initial contract set)";
    process.stdout.write(`Codex contract check passed${comparison}.\n`);
    return;
  }

  for (const error of validationErrors) {
    process.stderr.write(`Contract validation: ${error}\n`);
  }
  for (const change of breakingChanges) {
    process.stderr.write(`Breaking v1 change: ${change}\n`);
  }
  process.exitCode = 1;
}

main();
