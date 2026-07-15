import { execFileSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(fileURLToPath(new URL(".", import.meta.url)), "..");

const TRACKED_PATHS = [
  "contracts/codex/generated/contract-bundle.v1.json",
  "contracts/codex/generated/contract-bundle.v1.sha256",
  "contracts/codex/generated/capability-manifest.types.ts",
  "contracts/codex/generated/feature-policy.types.ts",
  "crates/codex-contracts/src/generated.rs",
];

function run(command, args, options = {}) {
  return execFileSync(command, args, {
    cwd: ROOT,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
    ...options,
  });
}

function main() {
  run("npm", ["run", "generate:codex-contracts"]);
  run("npm", ["run", "generate:codex-consumer-types"]);

  const diff = run("git", ["diff", "--", ...TRACKED_PATHS]);
  if (diff.trim().length > 0) {
    process.stderr.write("Generated Codex contract artifacts are out of date:\n");
    process.stderr.write(diff);
    process.stderr.write(
      "\nRun `npm run generate:codex-contracts && npm run generate:codex-consumer-types` and commit the result.\n",
    );
    process.exitCode = 1;
    return;
  }

  process.stdout.write("Codex generated artifact drift check passed.\n");
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
