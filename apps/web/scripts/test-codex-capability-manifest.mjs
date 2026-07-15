import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  negotiateCapabilityManifest,
  parseCapabilityManifest,
} from "./lib/codex-capability-manifest.mjs";

const fixture = JSON.parse(
  readFileSync(resolve("contracts/codex/fixtures/capability-manifest.v1.json"), "utf8"),
);
const registry = JSON.parse(
  readFileSync(resolve("contracts/codex/policy/capability-ids.v1.json"), "utf8"),
);

const parsed = parseCapabilityManifest(fixture);
if (parsed.byId.size !== registry.capabilities.length) {
  throw new Error(
    `expected ${registry.capabilities.length} registered capabilities, received ${parsed.byId.size}`,
  );
}

const compatible = negotiateCapabilityManifest(fixture, {
  clientProtocolVersion: "1.0.0",
  allowedServerBuilds: ["fixture-v1"],
  requiredCapabilities: ["protocol.initialize", "thread.lifecycle"],
});
if (compatible.status !== "compatible") {
  throw new Error(`expected compatible result: ${compatible.reasons.join(", ")}`);
}
if (compatible.capabilities.get("agents.multi_agent").effectiveStatus !== "unsupported") {
  throw new Error("experimental capabilities must be disabled by default");
}

const experimental = negotiateCapabilityManifest(fixture, {
  clientProtocolVersion: "1.0.0",
  allowedServerBuilds: ["fixture-v1"],
  allowExperimental: ["agents.multi_agent"],
});
if (experimental.capabilities.get("agents.multi_agent").effectiveStatus !== "supported") {
  throw new Error("explicitly allowed experimental capability was not enabled");
}

const incompatible = negotiateCapabilityManifest(fixture, {
  clientProtocolVersion: "2.0.0",
  allowedServerBuilds: ["other-build"],
});
if (incompatible.status !== "incompatible" || incompatible.reasons.length !== 2) {
  throw new Error("protocol/build incompatibility was not detected");
}

const withUnknownField = structuredClone(fixture);
withUnknownField.futureField = { nested: true };
parseCapabilityManifest(withUnknownField);

const duplicate = structuredClone(fixture);
duplicate.capabilities.push(structuredClone(duplicate.capabilities[0]));
try {
  parseCapabilityManifest(duplicate);
  throw new Error("duplicate capability unexpectedly passed");
} catch (error) {
  if (!String(error).includes("duplicate capability")) {
    throw error;
  }
}

process.stdout.write("Codex Capability Manifest tests passed.\n");
