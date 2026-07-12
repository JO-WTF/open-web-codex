import { execFileSync } from "node:child_process";

const ADDED_PACKAGING_PATHS = [
  /^src-tauri\/icons\//,
  /^src-tauri\/gen\//,
  /^src-tauri\/capabilities\//,
  /^src-tauri\/tauri(?:\.[^/]+)?\.conf\.json$/,
  /^scripts\/.*(?:ios|testflight|appimage|dmg|msi|nsis).*$/i,
];

function parseArgs(argv) {
  const options = { base: null, selfTest: false, workingTree: false };
  for (let index = 0; index < argv.length; index += 1) {
    const value = argv[index];
    if (value === "--self-test") {
      options.selfTest = true;
    } else if (value === "--working-tree") {
      options.workingTree = true;
    } else if (value === "--base") {
      options.base = argv[index + 1] ?? null;
      index += 1;
    } else {
      throw new Error(`Unknown argument: ${value}`);
    }
  }
  return options;
}

function runGit(args) {
  return execFileSync("git", args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  }).trimEnd();
}

function resolveDiffBase(requestedBase) {
  if (!requestedBase || /^0+$/.test(requestedBase)) {
    try {
      runGit(["rev-parse", "HEAD^"]);
      return "HEAD^";
    } catch {
      return "HEAD";
    }
  }
  runGit(["rev-parse", "--verify", `${requestedBase}^{commit}`]);
  return runGit(["merge-base", requestedBase, "HEAD"]);
}

function isFrontendSource(path) {
  return /^src\/.*\.(?:js|jsx|ts|tsx)$/.test(path);
}

function isRustSource(path) {
  return /^src-tauri\/.*\.rs$/.test(path);
}

function inspectAddedLine(path, line) {
  const violations = [];
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("//") || trimmed.startsWith("# ")) {
    return violations;
  }

  if (isFrontendSource(path) && line.includes("@tauri-apps/")) {
    violations.push("new frontend @tauri-apps import");
  }

  if (isRustSource(path) && /\btauri(?:::|_)/.test(line)) {
    violations.push("new direct Tauri Rust dependency");
  }

  if (
    path === "src-tauri/src/lib.rs" &&
    /^\s*[a-z_][a-z0-9_]*::[A-Za-z_][A-Za-z0-9_]*,?\s*$/.test(line)
  ) {
    violations.push("new Tauri command registration");
  }

  if (
    path === "package.json" &&
    /["'](?:@tauri-apps\/|tauri-plugin-|tauri(?=["']))/i.test(line)
  ) {
    violations.push("new or changed Tauri package/script entry");
  }

  if (
    path.endsWith("Cargo.toml") &&
    /^\s*(?:tauri|tauri-plugin-[A-Za-z0-9_-]+)\s*=/.test(line)
  ) {
    violations.push("new or changed Tauri Cargo dependency");
  }

  if (
    /^\.github\/workflows\/.*\.ya?ml$/.test(path) &&
    /(?:npm\s+run\s+tauri|npx\s+tauri|\btauri\s+(?:build|dev|ios|android)|src-tauri\/tauri\.)/i.test(
      line,
    )
  ) {
    violations.push("new Tauri CI or release step");
  }

  return violations;
}

export function inspectDiff(diffText, nameStatusText = "") {
  const findings = [];
  let currentPath = null;
  let newLine = 0;

  for (const line of diffText.split(/\r?\n/)) {
    if (line.startsWith("+++ b/")) {
      currentPath = line.slice(6);
      continue;
    }
    if (line.startsWith("@@")) {
      const match = line.match(/\+(\d+)/);
      newLine = match ? Number(match[1]) : 0;
      continue;
    }
    if (!currentPath || line.startsWith("---")) {
      continue;
    }
    if (line.startsWith("+") && !line.startsWith("+++")) {
      const content = line.slice(1);
      for (const reason of inspectAddedLine(currentPath, content)) {
        findings.push({ path: currentPath, line: newLine, reason, content });
      }
      newLine += 1;
    } else if (!line.startsWith("-")) {
      newLine += 1;
    }
  }

  for (const line of nameStatusText.split(/\r?\n/)) {
    const [status, path] = line.split("\t");
    if (status === "A" && path && ADDED_PACKAGING_PATHS.some((rule) => rule.test(path))) {
      findings.push({
        path,
        line: 0,
        reason: "new desktop/mobile packaging file",
        content: "",
      });
    }
  }

  return findings;
}

function runSelfTest() {
  const blocked = inspectDiff(
    [
      "diff --git a/src/new.ts b/src/new.ts",
      "--- a/src/new.ts",
      "+++ b/src/new.ts",
      "@@ -0,0 +1 @@",
      '+import { invoke } from "@tauri-apps/api/core";',
    ].join("\n"),
  );
  const removal = inspectDiff(
    [
      "diff --git a/src/old.ts b/src/old.ts",
      "--- a/src/old.ts",
      "+++ b/src/old.ts",
      "@@ -1 +0,0 @@",
      '-import { invoke } from "@tauri-apps/api/core";',
    ].join("\n"),
  );
  const packaging = inspectDiff("", "A\tsrc-tauri/icons/new-icon.png");
  const blockedRust = inspectDiff(
    [
      "diff --git a/src-tauri/src/new.rs b/src-tauri/src/new.rs",
      "--- a/src-tauri/src/new.rs",
      "+++ b/src-tauri/src/new.rs",
      "@@ -0,0 +1 @@",
      "+#[tauri::command]",
    ].join("\n"),
  );
  const blockedWorkflow = inspectDiff(
    [
      "diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml",
      "--- a/.github/workflows/ci.yml",
      "+++ b/.github/workflows/ci.yml",
      "@@ -0,0 +1 @@",
      "+        run: npm run tauri -- build",
    ].join("\n"),
  );
  const allowedBoundaryWorkflow = inspectDiff(
    [
      "diff --git a/.github/workflows/ci.yml b/.github/workflows/ci.yml",
      "--- a/.github/workflows/ci.yml",
      "+++ b/.github/workflows/ci.yml",
      "@@ -0,0 +1,2 @@",
      "+  tauri-boundary:",
      "+    run: npm run check:tauri-boundary",
    ].join("\n"),
  );

  if (
    blocked.length !== 1 ||
    removal.length !== 0 ||
    packaging.length !== 1 ||
    blockedRust.length !== 1 ||
    blockedWorkflow.length !== 1 ||
    allowedBoundaryWorkflow.length !== 0
  ) {
    throw new Error("Tauri boundary self-test failed");
  }
  process.stdout.write("Tauri boundary self-test passed.\n");
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  if (options.selfTest) {
    runSelfTest();
    return;
  }

  const base = resolveDiffBase(options.base);
  const range = options.workingTree ? "HEAD" : `${base}..HEAD`;
  const diff = runGit([
    "diff",
    "--unified=0",
    "--no-color",
    "--diff-filter=ACMR",
    range,
    "--",
    "src",
    "src-tauri",
    "package.json",
    ".github/workflows",
    "scripts",
  ]);
  const nameStatus = runGit([
    "diff",
    "--name-status",
    "--diff-filter=A",
    range,
    "--",
    "src-tauri",
    "scripts",
  ]);
  const findings = inspectDiff(diff, nameStatus);

  if (findings.length === 0) {
    process.stdout.write(`Tauri boundary check passed for ${range}.\n`);
    return;
  }

  process.stderr.write("New desktop/Tauri coupling is not allowed:\n");
  for (const finding of findings) {
    const location = finding.line > 0 ? `${finding.path}:${finding.line}` : finding.path;
    process.stderr.write(`- ${location}: ${finding.reason}\n`);
    if (finding.content) {
      process.stderr.write(`  ${finding.content.trim()}\n`);
    }
  }
  process.exitCode = 1;
}

main();
