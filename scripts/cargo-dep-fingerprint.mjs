#!/usr/bin/env node

import { createHash } from "node:crypto";
import {
  chmodSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, isAbsolute, join, relative, resolve, sep } from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const STAMP_SCHEMA = 1;
const BUILD_ENV_KEYS = [
  "AR",
  "CARGO_BUILD_TARGET",
  "CARGO_ENCODED_RUSTFLAGS",
  "CARGO_TARGET_DIR",
  "CARGO",
  "CC",
  "CFLAGS",
  "CODEX_BWRAP_SOURCE_DIR",
  "CODEX_SKIP_BWRAP_BUILD",
  "MACOSX_DEPLOYMENT_TARGET",
  "RUSTFLAGS",
  "RUSTC",
  "RUSTUP_TOOLCHAIN",
  "SDKROOT",
];

function fail(message) {
  throw new Error(message);
}

function parseArgs(argv) {
  const [command, ...rest] = argv;
  if (command !== "check" && command !== "record") {
    fail("usage: cargo-dep-fingerprint.mjs <check|record> [options]");
  }
  const options = {};
  for (let index = 0; index < rest.length; index += 2) {
    const key = rest[index];
    const value = rest[index + 1];
    if (!key?.startsWith("--") || value === undefined) {
      fail(`invalid argument: ${key ?? "<missing>"}`);
    }
    options[key.slice(2)] = value;
  }
  for (const key of [
    "workspace",
    "dep-info",
    "artifact",
    "stamp",
    "component",
    "profile",
    "build-command",
  ]) {
    if (!options[key]) {
      fail(`--${key} is required`);
    }
  }
  return { command, options };
}

export function parseCargoDepInfo(content, workspace) {
  let separator = -1;
  for (let index = 0; index < content.length - 1; index += 1) {
    if (content[index] === ":" && /\s/.test(content[index + 1])) {
      separator = index;
      break;
    }
  }
  if (separator === -1) {
    fail("Cargo dep-info has no dependency separator");
  }

  const dependencies = [];
  let token = "";
  let escaped = false;
  const source = content.slice(separator + 1);
  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    if (escaped) {
      if (character !== "\n" && character !== "\r") {
        token += character;
      }
      escaped = false;
    } else if (character === "\\") {
      escaped = true;
    } else if (/\s/.test(character)) {
      if (token) {
        dependencies.push(token);
        token = "";
      }
    } else {
      token += character;
    }
  }
  if (escaped) {
    token += "\\";
  }
  if (token) {
    dependencies.push(token);
  }

  return dependencies.map((dependency) =>
    resolve(workspace, dependency),
  );
}

function isWithin(root, candidate) {
  const path = relative(root, candidate);
  return path === "" || (!path.startsWith(`..${sep}`) && path !== ".." && !isAbsolute(path));
}

function nearestManifest(workspace, sourcePath) {
  if (!isWithin(workspace, sourcePath)) {
    return null;
  }
  let directory = dirname(sourcePath);
  while (isWithin(workspace, directory)) {
    const manifest = join(directory, "Cargo.toml");
    if (existsSync(manifest)) {
      return manifest;
    }
    if (directory === workspace) {
      break;
    }
    directory = dirname(directory);
  }
  return null;
}

function commandVersion(command, args, cwd) {
  const result = spawnSync(command, args, {
    cwd,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.status !== 0) {
    fail(`${command} version check failed: ${result.stderr.trim()}`);
  }
  return result.stdout.trim();
}

function defaultBuildContext(options) {
  const environment = Object.fromEntries(
    BUILD_ENV_KEYS.map((key) => [key, process.env[key] ?? ""]),
  );
  return {
    component: options.component,
    profile: options.profile,
    build_command: options["build-command"],
    platform: process.platform,
    architecture: process.arch,
    cargo: commandVersion("cargo", ["--version"], options.workspace),
    rustc: commandVersion("rustc", ["-vV"], options.workspace),
    environment,
  };
}

function fileIdentity(workspace, file) {
  return isWithin(workspace, file) ? relative(workspace, file) : file;
}

function updateField(hash, label, value) {
  hash.update(label);
  hash.update("\0");
  hash.update(value);
  hash.update("\0");
}

function collectDirectoryInputs(path, files, directories, symlinks) {
  const canonicalPath = realpathSync(path);
  directories.add(canonicalPath);
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    const child = join(path, entry.name);
    if (entry.isSymbolicLink()) {
      symlinks.set(resolve(child), readlinkSync(child));
    } else if (entry.isDirectory()) {
      collectDirectoryInputs(child, files, directories, symlinks);
    } else if (entry.isFile()) {
      files.add(realpathSync(child));
    }
  }
}

export function computeInputFingerprint(rawOptions, buildContext = null) {
  const workspace = realpathSync(rawOptions.workspace);
  const depInfo = resolve(rawOptions["dep-info"]);
  if (!existsSync(depInfo)) {
    fail(`Cargo dep-info is missing: ${depInfo}`);
  }
  const dependencies = parseCargoDepInfo(
    readFileSync(depInfo, "utf8"),
    workspace,
  );
  const files = new Set();
  const directories = new Set();
  const symlinks = new Map();
  for (const dependency of dependencies) {
    if (!existsSync(dependency)) {
      fail(`Cargo dependency is missing: ${dependency}`);
    }
    const dependencyStat = lstatSync(dependency);
    const canonicalDependency = realpathSync(dependency);
    if (dependencyStat.isDirectory()) {
      collectDirectoryInputs(dependency, files, directories, symlinks);
    } else if (dependencyStat.isSymbolicLink()) {
      symlinks.set(resolve(dependency), readlinkSync(dependency));
    } else if (dependencyStat.isFile()) {
      files.add(canonicalDependency);
    } else {
      fail(`Cargo dependency has an unsupported file type: ${dependency}`);
    }
    const manifest = nearestManifest(workspace, canonicalDependency);
    if (manifest) {
      files.add(realpathSync(manifest));
    }
  }
  for (const relativePath of [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    join(".cargo", "config.toml"),
  ]) {
    const path = join(workspace, relativePath);
    if (existsSync(path) && lstatSync(path).isFile()) {
      files.add(realpathSync(path));
    }
  }

  const sortedFiles = [...files].sort((left, right) =>
    fileIdentity(workspace, left).localeCompare(fileIdentity(workspace, right)),
  );
  const sortedDirectories = [...directories].sort((left, right) =>
    fileIdentity(workspace, left).localeCompare(fileIdentity(workspace, right)),
  );
  const sortedSymlinks = [...symlinks].sort(([left], [right]) =>
    fileIdentity(workspace, left).localeCompare(fileIdentity(workspace, right)),
  );
  const hash = createHash("sha256");
  updateField(hash, "schema", String(STAMP_SCHEMA));
  updateField(
    hash,
    "build-context",
    JSON.stringify(buildContext ?? defaultBuildContext(rawOptions)),
  );
  for (const directory of sortedDirectories) {
    updateField(hash, "directory", fileIdentity(workspace, directory));
  }
  for (const [path, target] of sortedSymlinks) {
    updateField(hash, "symlink-path", fileIdentity(workspace, path));
    updateField(hash, "symlink-target", target);
  }
  for (const file of sortedFiles) {
    updateField(hash, "path", fileIdentity(workspace, file));
    updateField(hash, "content", readFileSync(file));
  }
  return {
    input_sha256: hash.digest("hex"),
    dependency_count:
      sortedFiles.length + sortedDirectories.length + sortedSymlinks.length,
  };
}

function artifactState(path) {
  if (!existsSync(path)) {
    fail(`artifact is missing: ${path}`);
  }
  const stat = statSync(path, { bigint: true });
  if (!stat.isFile() || (stat.mode & 0o111n) === 0n) {
    fail(`artifact is not an executable file: ${path}`);
  }
  return {
    path: resolve(path),
    size: stat.size.toString(),
    mtime_ns: stat.mtimeNs.toString(),
  };
}

export function recordFingerprint(rawOptions, buildContext = null) {
  const fingerprint = computeInputFingerprint(rawOptions, buildContext);
  const artifact = artifactState(rawOptions.artifact);
  const stamp = {
    schema: STAMP_SCHEMA,
    component: rawOptions.component,
    profile: rawOptions.profile,
    ...fingerprint,
    artifact,
  };
  const stampPath = resolve(rawOptions.stamp);
  mkdirSync(dirname(stampPath), { recursive: true, mode: 0o700 });
  const temporaryPath = `${stampPath}.tmp-${process.pid}`;
  try {
    writeFileSync(temporaryPath, `${JSON.stringify(stamp, null, 2)}\n`, {
      mode: 0o600,
      flag: "wx",
    });
    renameSync(temporaryPath, stampPath);
    chmodSync(stampPath, 0o600);
  } finally {
    rmSync(temporaryPath, { force: true });
  }
  return stamp;
}

export function checkFingerprint(rawOptions, buildContext = null) {
  const stampPath = resolve(rawOptions.stamp);
  if (!existsSync(stampPath)) {
    return { fresh: false, reason: "stamp is missing" };
  }
  let stamp;
  try {
    stamp = JSON.parse(readFileSync(stampPath, "utf8"));
  } catch {
    return { fresh: false, reason: "stamp is invalid" };
  }
  if (
    stamp.schema !== STAMP_SCHEMA ||
    stamp.component !== rawOptions.component ||
    stamp.profile !== rawOptions.profile
  ) {
    return { fresh: false, reason: "stamp identity changed" };
  }

  let artifact;
  let fingerprint;
  try {
    artifact = artifactState(rawOptions.artifact);
    fingerprint = computeInputFingerprint(rawOptions, buildContext);
  } catch (error) {
    return {
      fresh: false,
      reason: error instanceof Error ? error.message : String(error),
    };
  }
  if (
    stamp.artifact?.path !== artifact.path ||
    stamp.artifact?.size !== artifact.size ||
    stamp.artifact?.mtime_ns !== artifact.mtime_ns
  ) {
    return { fresh: false, reason: "artifact identity changed" };
  }
  if (stamp.input_sha256 !== fingerprint.input_sha256) {
    return { fresh: false, reason: "Cargo build inputs changed" };
  }
  return {
    fresh: true,
    reason: `${fingerprint.dependency_count} exact Cargo inputs matched`,
  };
}

function main() {
  const { command, options } = parseArgs(process.argv.slice(2));
  if (command === "record") {
    const stamp = recordFingerprint(options);
    console.log(
      `recorded ${stamp.component}: ${stamp.dependency_count} exact Cargo inputs`,
    );
    return;
  }
  const result = checkFingerprint(options);
  console.log(`${result.fresh ? "fresh" : "stale"}: ${options.component}: ${result.reason}`);
  if (!result.fresh) {
    process.exitCode = 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}
