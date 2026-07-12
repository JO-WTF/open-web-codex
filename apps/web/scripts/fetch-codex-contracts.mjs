import { createHash } from "node:crypto";
import {
  access,
  mkdir,
  mkdtemp,
  readFile,
  rename,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, isAbsolute, join, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const MAX_BUNDLE_BYTES = 50 * 1024 * 1024;
const MAX_FILE_BYTES = 10 * 1024 * 1024;
const SAFE_FILE = /^(?:manifest|schemas|fixtures)\/[A-Za-z0-9._/-]+\.json$/;

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function parseArgs(argv) {
  const options = {
    source: null,
    expectedSha256: null,
    cacheDir: resolve(".cache/codex-contracts"),
    selfTest: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--source") {
      options.source = argv[index + 1];
      index += 1;
    } else if (value === "--sha256") {
      options.expectedSha256 = argv[index + 1]?.toLowerCase() ?? null;
      index += 1;
    } else if (value === "--cache-dir") {
      options.cacheDir = resolve(argv[index + 1]);
      index += 1;
    } else if (value === "--self-test") {
      options.selfTest = true;
    } else {
      throw new Error(`Unknown argument: ${value}`);
    }
  }
  return options;
}

async function readSource(source) {
  if (/^https:\/\//i.test(source)) {
    const response = await fetch(source, { redirect: "error" });
    if (!response.ok) {
      throw new Error(`contract download failed with HTTP ${response.status}`);
    }
    const declaredLength = Number(response.headers.get("content-length") ?? 0);
    if (declaredLength > MAX_BUNDLE_BYTES) {
      throw new Error("contract bundle exceeds the maximum size");
    }
    const bytes = Buffer.from(await response.arrayBuffer());
    if (bytes.length > MAX_BUNDLE_BYTES) {
      throw new Error("contract bundle exceeds the maximum size");
    }
    return bytes;
  }
  if (/^[a-z]+:\/\//i.test(source)) {
    throw new Error("only HTTPS URLs or local paths are allowed");
  }
  const bytes = await readFile(resolve(source));
  if (bytes.length > MAX_BUNDLE_BYTES) {
    throw new Error("contract bundle exceeds the maximum size");
  }
  return bytes;
}

function assertSafePath(path) {
  if (
    typeof path !== "string" ||
    !SAFE_FILE.test(path) ||
    isAbsolute(path) ||
    path.includes("\\") ||
    path.split("/").includes("..")
  ) {
    throw new Error(`unsafe contract bundle path: ${path}`);
  }
}

export function verifyContractBundle(bytes, expectedSha256) {
  const bundleHash = sha256(bytes);
  if (!expectedSha256 || !/^[a-f0-9]{64}$/.test(expectedSha256)) {
    throw new Error("a valid expected SHA-256 is required");
  }
  if (bundleHash !== expectedSha256) {
    throw new Error(`contract bundle SHA-256 mismatch: expected ${expectedSha256}, got ${bundleHash}`);
  }

  let bundle;
  try {
    bundle = JSON.parse(bytes.toString("utf8"));
  } catch (error) {
    throw new Error(`contract bundle is not valid JSON: ${error.message}`);
  }
  if (!/^[0-9]+\.[0-9]+\.[0-9]+$/.test(bundle.bundleVersion ?? "")) {
    throw new Error("contract bundleVersion must be SemVer");
  }
  if (!bundle.build?.version || !bundle.build?.commit || !bundle.build?.target) {
    throw new Error("contract bundle build identity is incomplete");
  }
  if (!Array.isArray(bundle.files) || bundle.files.length === 0) {
    throw new Error("contract bundle files must be a non-empty array");
  }

  const seen = new Set();
  const files = bundle.files.map((file) => {
    assertSafePath(file.path);
    if (seen.has(file.path)) {
      throw new Error(`duplicate contract bundle path: ${file.path}`);
    }
    seen.add(file.path);
    if (!/^[a-f0-9]{64}$/.test(file.sha256 ?? "")) {
      throw new Error(`invalid file SHA-256 for ${file.path}`);
    }
    const content = Buffer.from(file.contentBase64 ?? "", "base64");
    if (content.length === 0 || content.length > MAX_FILE_BYTES) {
      throw new Error(`invalid decoded size for ${file.path}`);
    }
    const actual = sha256(content);
    if (actual !== file.sha256) {
      throw new Error(`file SHA-256 mismatch for ${file.path}`);
    }
    JSON.parse(content.toString("utf8"));
    return { path: file.path, content };
  });
  return { bundle, bundleHash, files };
}

export async function fetchContractBundle(options) {
  const bytes = await readSource(options.source);
  const verified = verifyContractBundle(bytes, options.expectedSha256);
  const finalDir = join(options.cacheDir, verified.bundleHash);
  try {
    await access(join(finalDir, "bundle.json"));
    return { cacheDir: finalDir, cacheHit: true, ...verified };
  } catch {
    // Continue with an atomic cache population.
  }

  await mkdir(options.cacheDir, { recursive: true });
  const tempDir = await mkdtemp(join(options.cacheDir, ".tmp-"));
  try {
    await writeFile(join(tempDir, "bundle.json"), bytes, { flag: "wx" });
    for (const file of verified.files) {
      const destination = join(tempDir, ...file.path.split("/"));
      const relative = destination.slice(tempDir.length + 1);
      if (relative.startsWith(`..${sep}`) || isAbsolute(relative)) {
        throw new Error(`contract path escapes cache: ${file.path}`);
      }
      await mkdir(dirname(destination), { recursive: true });
      await writeFile(destination, file.content, { flag: "wx" });
    }
    await rename(tempDir, finalDir);
  } catch (error) {
    await rm(tempDir, { recursive: true, force: true });
    throw error;
  }
  return { cacheDir: finalDir, cacheHit: false, ...verified };
}

async function runSelfTest() {
  const root = await mkdtemp(join(tmpdir(), "codex-contract-fetch-"));
  try {
    const content = Buffer.from(JSON.stringify({ schemaVersion: "1.0.0" }));
    const bundle = {
      bundleVersion: "1.0.0",
      createdAt: "2026-07-12T00:00:00Z",
      build: { version: "fixture-v1", commit: "0000000", target: "test-target" },
      files: [
        {
          path: "manifest/capability-manifest.json",
          sha256: sha256(content),
          contentBase64: content.toString("base64"),
        },
      ],
    };
    const bytes = Buffer.from(JSON.stringify(bundle));
    const source = join(root, "bundle.json");
    await writeFile(source, bytes);
    const options = {
      source,
      expectedSha256: sha256(bytes),
      cacheDir: join(root, "cache"),
    };
    const first = await fetchContractBundle(options);
    const second = await fetchContractBundle(options);
    if (first.cacheHit || !second.cacheHit) {
      throw new Error("contract bundle cache behavior is incorrect");
    }

    const unsafe = structuredClone(bundle);
    unsafe.files[0].path = "../escape.json";
    const unsafeBytes = Buffer.from(JSON.stringify(unsafe));
    try {
      verifyContractBundle(unsafeBytes, sha256(unsafeBytes));
      throw new Error("unsafe bundle path unexpectedly passed");
    } catch (error) {
      if (!String(error).includes("unsafe contract bundle path")) {
        throw error;
      }
    }
  } finally {
    await rm(root, { recursive: true, force: true });
  }
  process.stdout.write("Codex contract fetch self-test passed.\n");
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.selfTest) {
    await runSelfTest();
    return;
  }
  if (!options.source || !options.expectedSha256) {
    throw new Error("--source and --sha256 are required");
  }
  const result = await fetchContractBundle(options);
  process.stdout.write(
    `${JSON.stringify({ cacheDir: result.cacheDir, cacheHit: result.cacheHit, sha256: result.bundleHash })}\n`,
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
