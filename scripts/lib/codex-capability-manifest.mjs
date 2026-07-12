const SEMVER = /^([0-9]+)\.([0-9]+)\.([0-9]+)$/;
const STATUSES = new Set([
  "supported",
  "unsupported",
  "degraded",
  "incompatible",
  "experimental",
]);

function parseVersion(value, field) {
  const match = typeof value === "string" ? value.match(SEMVER) : null;
  if (!match) {
    throw new Error(`${field} must be SemVer`);
  }
  return match.slice(1).map(Number);
}

function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) {
      return left[index] < right[index] ? -1 : 1;
    }
  }
  return 0;
}

export function parseCapabilityManifest(input) {
  const manifest = typeof input === "string" ? JSON.parse(input) : structuredClone(input);
  const [schemaMajor] = parseVersion(manifest?.schemaVersion, "schemaVersion");
  if (schemaMajor !== 1) {
    throw new Error(`unsupported Capability Manifest schema major ${schemaMajor}`);
  }
  parseVersion(manifest?.server?.protocolVersion, "server.protocolVersion");
  parseVersion(manifest?.compatibility?.minimumClientProtocol, "minimumClientProtocol");
  parseVersion(manifest?.compatibility?.maximumClientProtocol, "maximumClientProtocol");
  if (!Array.isArray(manifest.capabilities)) {
    throw new Error("capabilities must be an array");
  }

  const byId = new Map();
  for (const capability of manifest.capabilities) {
    if (typeof capability.id !== "string" || !capability.id.includes(".")) {
      throw new Error("capability id is invalid");
    }
    if (byId.has(capability.id)) {
      throw new Error(`duplicate capability id ${capability.id}`);
    }
    parseVersion(capability.version, `${capability.id}.version`);
    if (!STATUSES.has(capability.status)) {
      throw new Error(`${capability.id} has unknown status ${capability.status}`);
    }
    if (
      ["unsupported", "degraded", "incompatible"].includes(capability.status) &&
      (!capability.reason?.code || !capability.reason?.message)
    ) {
      throw new Error(`${capability.id} requires a structured reason`);
    }
    byId.set(capability.id, capability);
  }
  return { manifest, byId };
}

export function negotiateCapabilityManifest(input, policy) {
  const { manifest, byId } = parseCapabilityManifest(input);
  const client = parseVersion(policy.clientProtocolVersion, "policy.clientProtocolVersion");
  const minimum = parseVersion(
    manifest.compatibility.minimumClientProtocol,
    "minimumClientProtocol",
  );
  const maximum = parseVersion(
    manifest.compatibility.maximumClientProtocol,
    "maximumClientProtocol",
  );
  const reasons = [];

  if (compareVersions(client, minimum) < 0 || compareVersions(client, maximum) > 0) {
    reasons.push("client protocol is outside the server compatibility range");
  }
  if (
    Array.isArray(policy.allowedServerBuilds) &&
    policy.allowedServerBuilds.length > 0 &&
    !policy.allowedServerBuilds.includes(manifest.server.buildVersion)
  ) {
    reasons.push(`server build ${manifest.server.buildVersion} is not allowlisted`);
  }

  const capabilities = new Map();
  for (const [id, capability] of byId) {
    let effectiveStatus = capability.status;
    if (capability.status === "experimental") {
      const allowed =
        policy.allowExperimental === true ||
        (Array.isArray(policy.allowExperimental) && policy.allowExperimental.includes(id));
      effectiveStatus = allowed ? "supported" : "unsupported";
    }
    capabilities.set(id, { ...capability, effectiveStatus });
  }

  for (const id of policy.requiredCapabilities ?? []) {
    const capability = capabilities.get(id);
    if (!capability || ["unsupported", "incompatible"].includes(capability.effectiveStatus)) {
      reasons.push(`required capability ${id} is unavailable`);
    }
  }

  return {
    status: reasons.length === 0 ? "compatible" : "incompatible",
    reasons,
    capabilities,
    server: manifest.server,
  };
}
