#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const WEB_ROOT = resolve(import.meta.dirname, "..");

function run(command, args, env = {}) {
  const result = spawnSync(command, args, {
    cwd: WEB_ROOT,
    env: { ...process.env, ...env },
    encoding: "utf8",
    stdio: "pipe",
  });
  if (result.status !== 0) {
    process.stderr.write(result.stdout ?? "");
    process.stderr.write(result.stderr ?? "");
    process.exit(result.status ?? 1);
  }
  return result.stdout;
}

run("rustup", ["run", "1.95.0", "cargo", "test", "-p", "open-web-codex-server", "-p", "open-web-codex-profile-host", "-p", "open-web-codex-adapter"]);
run("npm", ["run", "typecheck"]);
run("npm", ["test", "--", "src/services/platformClient.test.ts", "src/utils/projectedRunEventsToLogEntries.test.ts"]);
run("node", ["scripts/platform-batch23-smoke.mjs"], {
  PLATFORM_SMOKE_DB_PASSWORD: process.env.PLATFORM_SMOKE_DB_PASSWORD ?? "ubuntu",
});
run("node", ["scripts/platform-web-smoke.mjs"], {
  PLATFORM_SMOKE_DB_PASSWORD: process.env.PLATFORM_SMOKE_DB_PASSWORD ?? "ubuntu",
});

console.log("check:platform-web passed");
