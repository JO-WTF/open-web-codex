import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const GENERATED_DIR = resolve(ROOT, "contracts/codex/generated");

const INPUTS = [
  {
    source: "contracts/codex/capability-manifest.schema.json",
    bundlePath: "schemas/capability-manifest.schema.json",
  },
  {
    source: "contracts/codex/fixtures/capability-manifest.v1.json",
    bundlePath: "manifest/capability-manifest.json",
  },
  {
    source: "contracts/codex/fixtures/capability-manifest.v1.json",
    bundlePath: "fixtures/capability-manifest.v1.json",
  },
];

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function buildIdentity() {
  const version = process.env.CODEX_BUILD_VERSION ?? "local-dev";
  const commit = process.env.CODEX_BUILD_COMMIT ?? "unknown";
  const target = process.env.CODEX_BUILD_TARGET ?? "unknown-target";
  return { version, commit, target };
}

export function buildContractBundle({ createdAt = new Date().toISOString() } = {}) {
  const files = INPUTS.map(({ source, bundlePath }) => {
    const content = readFileSync(resolve(ROOT, source));
    return {
      path: bundlePath,
      sha256: sha256(content),
      contentBase64: content.toString("base64"),
    };
  });

  return {
    bundleVersion: "1.0.0",
    createdAt,
    build: buildIdentity(),
    files,
  };
}

export function writeContractBundle(bundle, outputDir = GENERATED_DIR) {
  mkdirSync(outputDir, { recursive: true });
  const bytes = Buffer.from(JSON.stringify(bundle));
  const bundlePath = resolve(outputDir, "contract-bundle.v1.json");
  const digestPath = resolve(outputDir, "contract-bundle.v1.sha256");
  writeFileSync(bundlePath, bytes);
  writeFileSync(digestPath, `${sha256(bytes)}\n`);
  return { bundlePath, digestPath, sha256: sha256(bytes) };
}

function main() {
  const bundle = buildContractBundle();
  const result = writeContractBundle(bundle);
  process.stdout.write(
    `${JSON.stringify({
      bundlePath: result.bundlePath,
      sha256: result.sha256,
      fileCount: bundle.files.length,
    })}\n`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main();
}
