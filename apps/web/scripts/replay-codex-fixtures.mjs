import { readFileSync, readdirSync } from "node:fs";
import { basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const DEFAULT_FIXTURE_DIR = resolve("contracts/codex/fixtures");

function parseArgs(argv) {
  const paths = [];
  let selfTest = false;
  for (let index = 0; index < argv.length; index += 1) {
    if (argv[index] === "--self-test") {
      selfTest = true;
    } else {
      paths.push(resolve(argv[index]));
    }
  }
  return { paths, selfTest };
}

function correlationKey(value) {
  return `${typeof value}:${String(value)}`;
}

function methodFrom(message) {
  return typeof message.payload?.method === "string" ? message.payload.method : null;
}

export function replayFixture(fixture, knownMethods = new Set()) {
  if (!fixture || typeof fixture !== "object") {
    throw new Error("Fixture must be an object");
  }
  if (!Array.isArray(fixture.messages) || fixture.messages.length === 0) {
    throw new Error("Fixture messages must be a non-empty array");
  }

  const pendingClientRequests = new Map();
  const pendingServerRequests = new Map();
  const seenCorrelations = new Set();
  const summary = {
    messages: 0,
    requests: 0,
    responses: 0,
    notifications: 0,
    serverRequests: 0,
    unknownNotifications: 0,
    unknownServerRequests: 0,
  };

  for (const [index, message] of fixture.messages.entries()) {
    summary.messages += 1;
    if (!message || typeof message !== "object" || typeof message.payload !== "object") {
      throw new Error(`message ${index} must contain an object payload`);
    }

    const method = methodFrom(message);
    if (message.kind === "request") {
      if (message.direction !== "client_to_server" || !method) {
        throw new Error(`message ${index} has an invalid client request`);
      }
      const key = correlationKey(message.correlation);
      if (seenCorrelations.has(`client:${key}`)) {
        throw new Error(`duplicate client request correlation at message ${index}`);
      }
      seenCorrelations.add(`client:${key}`);
      pendingClientRequests.set(key, { index, method });
      summary.requests += 1;
    } else if (message.kind === "server_request") {
      if (message.direction !== "server_to_client" || !method) {
        throw new Error(`message ${index} has an invalid server request`);
      }
      const key = correlationKey(message.correlation);
      if (seenCorrelations.has(`server:${key}`)) {
        throw new Error(`duplicate server request correlation at message ${index}`);
      }
      seenCorrelations.add(`server:${key}`);
      pendingServerRequests.set(key, { index, method });
      summary.serverRequests += 1;
      if (knownMethods.size > 0 && !knownMethods.has(method)) {
        summary.unknownServerRequests += 1;
      }
    } else if (message.kind === "response") {
      const key = correlationKey(message.correlation);
      const pending =
        message.direction === "server_to_client"
          ? pendingClientRequests
          : message.direction === "client_to_server"
            ? pendingServerRequests
            : null;
      if (!pending || !pending.has(key)) {
        throw new Error(`message ${index} responds to an unknown correlation`);
      }
      const request = pending.get(key);
      if (!("result" in message.payload) && !("error" in message.payload)) {
        throw new Error(`message ${index} response has neither result nor error`);
      }
      if (
        message.direction === "client_to_server" &&
        knownMethods.size > 0 &&
        !knownMethods.has(request.method) &&
        !("error" in message.payload)
      ) {
        throw new Error(`unknown server request ${request.method} must receive an explicit error`);
      }
      pending.delete(key);
      summary.responses += 1;
    } else if (message.kind === "notification") {
      if (!method || message.correlation !== undefined) {
        throw new Error(`message ${index} has an invalid notification`);
      }
      summary.notifications += 1;
      if (knownMethods.size > 0 && !knownMethods.has(method)) {
        summary.unknownNotifications += 1;
      }
    } else {
      throw new Error(`message ${index} has unknown kind ${message.kind}`);
    }
  }

  if (pendingClientRequests.size > 0 || pendingServerRequests.size > 0) {
    throw new Error("Fixture ends with unresolved requests");
  }
  return summary;
}

function loadKnownMethods() {
  const manifest = JSON.parse(
    readFileSync(resolve(DEFAULT_FIXTURE_DIR, "capability-manifest.v1.json"), "utf8"),
  );
  const methods = new Set();
  for (const capability of manifest.capabilities ?? []) {
    for (const group of ["clientRequests", "serverRequests", "notifications"]) {
      for (const method of capability.methods?.[group] ?? []) {
        methods.add(method);
      }
    }
  }
  return methods;
}

function discoverFixtures(paths) {
  if (paths.length > 0) {
    return paths;
  }
  return readdirSync(DEFAULT_FIXTURE_DIR)
    .filter((name) => name.endsWith(".protocol.json"))
    .sort()
    .map((name) => resolve(DEFAULT_FIXTURE_DIR, name));
}

function runSelfTest() {
  const invalid = {
    messages: [
      {
        direction: "client_to_server",
        kind: "response",
        correlation: 1,
        payload: { id: 1, result: {} },
      },
    ],
  };
  try {
    replayFixture(invalid);
    throw new Error("invalid Fixture unexpectedly passed");
  } catch (error) {
    if (!String(error).includes("unknown correlation")) {
      throw error;
    }
  }
  process.stdout.write("Codex Fixture replay self-test passed.\n");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.selfTest) {
    runSelfTest();
    return;
  }
  const knownMethods = loadKnownMethods();
  const fixtures = discoverFixtures(options.paths);
  if (fixtures.length === 0) {
    throw new Error("No protocol Fixtures found");
  }
  for (const path of fixtures) {
    const fixture = JSON.parse(readFileSync(path, "utf8"));
    const summary = replayFixture(fixture, knownMethods);
    process.stdout.write(`${basename(path)} ${JSON.stringify(summary)}\n`);
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  main();
}
