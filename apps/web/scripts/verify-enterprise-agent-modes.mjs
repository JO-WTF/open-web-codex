#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const [, , profileRoot, evidenceFile] = process.argv;
if (!profileRoot || !evidenceFile) {
  throw new Error(
    "Usage: verify-enterprise-agent-modes.mjs <profile-root> <evidence-file>",
  );
}

const evidence = JSON.parse(fs.readFileSync(evidenceFile, "utf8"));
const rootThreadId = evidence.run?.rootThreadId;
if (!rootThreadId) {
  throw new Error("Enterprise evidence does not identify the root Supervisor Thread");
}

const childThreadIds = new Set(
  evidence.agents
    .map((agent) => agent.thread_id)
    .filter((threadId) => threadId && threadId !== rootThreadId),
);
const expectedThreadIds = new Set([rootThreadId, ...childThreadIds]);
const observedVersions = new Map();

function visit(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      visit(entryPath);
      continue;
    }
    if (!entry.name.endsWith(".jsonl")) continue;

    const firstLine = fs.readFileSync(entryPath, "utf8").split("\n", 1)[0];
    const event = JSON.parse(firstLine);
    if (event.type !== "session_meta") continue;

    const threadId = event.payload?.id;
    if (expectedThreadIds.has(threadId)) {
      observedVersions.set(threadId, event.payload?.multi_agent_version);
    }
  }
}

visit(path.join(profileRoot, "sessions"));

const failures = [];
if (observedVersions.get(rootThreadId) !== "v2") {
  failures.push(
    `${rootThreadId}=${observedVersions.get(rootThreadId) ?? "missing"} (root)`,
  );
}
for (const threadId of childThreadIds) {
  if (observedVersions.get(threadId) !== "disabled") {
    failures.push(
      `${threadId}=${observedVersions.get(threadId) ?? "missing"} (child)`,
    );
  }
}
if (failures.length > 0) {
  throw new Error(
    `Governed Agent collaboration modes did not match policy: ${failures.join(", ")}`,
  );
}

console.log(
  `Verified Multi-Agent V2 for the root Supervisor and disabled nested collaboration for ${childThreadIds.size} child Agents.`,
);
