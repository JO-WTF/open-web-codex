import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  negotiateCapabilityManifest,
  parseCapabilityManifest,
} from "./lib/codex-capability-manifest.mjs";

const fixture = JSON.parse(
  readFileSync(resolve("contracts/codex/fixtures/capability-manifest.v1.json"), "utf8"),
);
const featurePolicy = JSON.parse(
  readFileSync(resolve("contracts/codex/policy/feature-policy.v1.json"), "utf8"),
);

const parsed = parseCapabilityManifest(fixture);

const basePolicy = {
  clientProtocolVersion: "1.0.0",
  allowedServerBuilds: ["fixture-v1"],
};

function compareVersions(left, right) {
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) {
      return left[index] < right[index] ? -1 : 1;
    }
  }
  return 0;
}

function parseVersion(value) {
  return value.split(".").map(Number);
}

function resolveFeature(feature, negotiationPolicy) {
  const capability = parsed.byId.get(feature.capabilityId);
  if (!capability) {
    throw new Error(`feature ${feature.id} references missing capability ${feature.capabilityId}`);
  }

  const version = parseVersion(capability.version);
  const minimum = parseVersion(feature.minimumCapabilityVersion);
  if (compareVersions(version, minimum) < 0) {
    return { enabled: false, effectiveStatus: "incompatible-version" };
  }

  const negotiated = negotiateCapabilityManifest(fixture, negotiationPolicy);
  const effectiveStatus = negotiated.capabilities.get(feature.capabilityId)?.effectiveStatus;
  if (!effectiveStatus) {
    throw new Error(`negotiation did not return ${feature.capabilityId}`);
  }

  return {
    enabled: feature.allowedStatuses.includes(effectiveStatus),
    effectiveStatus,
  };
}

for (const feature of featurePolicy.features) {
  if (!feature.id || !feature.capabilityId) {
    throw new Error("feature policy entry is missing id or capabilityId");
  }
  if (!Array.isArray(feature.allowedStatuses) || feature.allowedStatuses.length === 0) {
    throw new Error(`feature ${feature.id} must declare allowedStatuses`);
  }
}

for (const feature of featurePolicy.features) {
  const resolution = resolveFeature(feature, basePolicy);
  if (feature.requiresExplicitEnable) {
    if (resolution.enabled) {
      throw new Error(`feature ${feature.id} must stay disabled without explicit enablement`);
    }
    const enabled = resolveFeature(feature, {
      ...basePolicy,
      allowExperimental: [feature.capabilityId],
    });
    if (!enabled.enabled) {
      throw new Error(`feature ${feature.id} did not enable with explicit experimental allowlist`);
    }
    continue;
  }

  if (!resolution.enabled) {
    throw new Error(
      `feature ${feature.id} is disabled but policy allows ${feature.allowedStatuses.join(", ")} for effective status ${resolution.effectiveStatus}`,
    );
  }
}

const requiredCapabilities = featurePolicy.features
  .filter((feature) => !feature.requiresExplicitEnable)
  .map((feature) => feature.capabilityId);
const compatible = negotiateCapabilityManifest(fixture, {
  ...basePolicy,
  requiredCapabilities,
});
if (compatible.status !== "compatible") {
  throw new Error(`expected compatible negotiation for P0 features: ${compatible.reasons.join(", ")}`);
}

process.stdout.write(`Feature policy tests passed for ${featurePolicy.features.length} P0 features.\n`);
