import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const harness = resolve("scripts/codex-app-server-smoke.mjs");
const fakeServer = resolve("scripts/fixtures/fake-codex-app-server.mjs");

function run(mode, timeoutMs = 1000) {
  return spawnSync(
    process.execPath,
    [
      harness,
      "--bin",
      process.execPath,
      "--bin-arg",
      fakeServer,
      "--bin-arg",
      `--mode=${mode}`,
      "--timeout-ms",
      String(timeoutMs),
    ],
    { encoding: "utf8" },
  );
}

const success = run("success");
if (success.status !== 0) {
  throw new Error(`success mode failed: ${success.stderr}`);
}
const successPayload = JSON.parse(success.stdout);
if (successPayload.ok !== true || successPayload.threadListCount !== 0) {
  throw new Error(`unexpected success payload: ${success.stdout}`);
}

const cases = [
  ["invalid-json", "invalid JSON from app-server", 1000],
  ["error", "initialize returned an error", 1000],
  ["invalid-initialize", "initialize response must contain string userAgent", 1000],
  ["thread-list-error", "thread/list returned an error", 1000],
  ["exit", "exited before official initialize/thread/list completed", 1000],
  ["timeout", "official initialize/thread/list timed out", 100],
];

for (const [mode, expected, timeoutMs] of cases) {
  const result = run(mode, timeoutMs);
  if (result.status === 0 || !result.stderr.includes(expected)) {
    throw new Error(`${mode} mode did not fail as expected: ${result.stderr}`);
  }
}

process.stdout.write("Codex app-server harness tests passed.\n");
