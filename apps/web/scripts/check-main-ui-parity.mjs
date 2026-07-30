import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { lstatSync, readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const webRoot = resolve(fileURLToPath(new URL("..", import.meta.url)));
const repoRoot = resolve(webRoot, "../..");
const sourceRoot = join(webRoot, "src");
const baseline = process.env.UI_PARITY_REF?.trim() || "main";
const uiOverlayRef = process.env.UI_OVERLAY_REF?.trim()
  || "9109c4c7a4687bee74b68d6360e49bc4a98c2c1a";
const interfaceSeams = new Set([
  "apps/web/src/services/webClient.ts",
  "apps/web/src/services/webClient.test.ts",
]);
// The overlay is the last reviewed UI snapshot. Files intentionally changed
// after that snapshot are pinned individually so a parity exception cannot
// silently grow into unrelated presentation drift.
const exactReviewedFiles = new Map([
  ["apps/web/src/WebApp.test.tsx", "6d026aa6f5415282a29c654ced6cbf42dba994635ec961b0678c8ccf0c98f93e"],
  ["apps/web/src/WebApp.tsx", "1eb0f76fe54eddd3a1c172eb82023463b36994420da70b5095a0dba9431f0d05"],
  ["apps/web/src/components/Conversation/Composer.tsx", "4a54f64e6c89f9756fbcfb9523639824e18551f3cddc3501af375260af02b261"],
  ["apps/web/src/components/Conversation/MessageList.test.tsx", "c5f3495600f8cbb147c72ef7fa19c7ef3fd7bad4bce3bba5eb2ce36e6251feaf"],
  ["apps/web/src/components/Conversation/MessageList.tsx", "6f97a2249ee3afb40bb7a7bd7c0c2d5023f633865429ff0d2f7766b284501ed8"],
  ["apps/web/src/components/Conversation/messages/AssistantMessage.test.tsx", "d2f2ff17a6a9c2187e700b0214032a0ec2b165744e563e695881752a8ca53aa9"],
  ["apps/web/src/components/Conversation/messages/AssistantMessage.tsx", "4f966a9df202742de62eb5dd0a3dc2d85bb51ca113bded409b56fcf096725845"],
  ["apps/web/src/components/Conversation/messages/ReplyCard.test.tsx", "3aa4b3ab682ccd1c86942f9c9097be1065fc04cf44cd1a89a0e632030cff9e7e"],
  ["apps/web/src/components/Conversation/messages/ReplyCard.tsx", "eda656d532c6f710cff98f396b04e0d2c32929a4615ea56db3629a278204bab6"],
  ["apps/web/src/components/Conversation/messages/ReportReplyCard.test.tsx", "17b2e0144aac3a07f2fc337a898ad946f19a15735a14d0361c6d7717abb9bff0"],
  ["apps/web/src/components/Conversation/messages/ReportReplyCard.tsx", "4127476db675bbcbeccfc1ec21b58a4a06f9b9870220f0c31e407e59dd13bcf3"],
  ["apps/web/src/components/Conversation/messages/SafeMarkdown.tsx", "1b96040f29cc4eeb4e8aeb40360569d5402ada4128f2f663b1f726566a85371c"],
  ["apps/web/src/components/Conversation/index.tsx", "119e3a4ef40e3b38ee927ee33ad95a872e8d72fd13f6dfc8fa9d1c62c2d07f33"],
  ["apps/web/src/components/FileManager/index.tsx", "dc13e8876259dadebbd5f177279512beac6c2225edb078f1760960f79f54a7d3"],
  ["apps/web/src/components/FileManager/index.test.tsx", "02287632a3062fd2e1bc475f2ea70ba1a0fdefd3fd2bbaed7e092850973a8d9e"],
  ["apps/web/src/components/Sidebar/AgentStudioDialog.tsx", "0dca928614c4b6eea8da0dfcffc83f34fe1e5bfb0bb0083b3fdffdaca9768e7a"],
  ["apps/web/src/components/Sidebar/LearnDialog.test.tsx", "a7562b3f7b20254a5b582ba05481ba96ed618588ef67839840bc1f8369f844f5"],
  ["apps/web/src/components/Sidebar/LearnDialog.tsx", "8b161b80b9c72099bfabbc81c844318f15dff69ebcdc9c434b51ae16c772c26b"],
  ["apps/web/src/components/Sidebar/McpStatus.test.tsx", "98f6eb2a9d2a3bf87810d2b6131d830b6721374c1cbc1c0649981ab97ec991cd"],
  ["apps/web/src/components/Sidebar/McpStatus.tsx", "ecddc1f687e06ea0394354245f3d3c29fa08e8f6e793550866f48a0d1f8e75c7"],
  ["apps/web/src/components/Sidebar/RunLauncherDialog.test.tsx", "4fbc61168c9eb98439ec7957e486612906e8358db48227a0ab02ae61b3663b11"],
  ["apps/web/src/components/Sidebar/RunLauncherDialog.tsx", "e0bd928a554d35e22eb3c7fb40c877f47431a891a0c0bf25f00f792065e89943"],
  ["apps/web/src/components/Sidebar/Workspaces.test.tsx", "53178bceadc389ca60babb3f901a8bd97aa05b438e8b78789afb3b51b82ce151"],
  ["apps/web/src/components/Sidebar/Workspaces.tsx", "0bdd32688a43476a1ee649022a9ba4780289e61d24f2a83c0d4c3d2353188150"],
  ["apps/web/src/components/Sidebar/index.test.tsx", "79d7bb6dc0ce005d9b6010eac3a4c08a3cda3c366894645c4e2169af3e98153f"],
  ["apps/web/src/components/Sidebar/index.tsx", "804bfb586ca8811fea00ce590f5da1876f3a20729903d3ea7824d47314251c57"],
  ["apps/web/src/features/app/hooks/useMainAppLayoutSurfaces.ts", "48b7a323ca22049108912d5f9cda520f24ae9787c7bdb02f7403bf6b6cd23414"],
  ["apps/web/src/features/files/components/DatasetReleaseDialog.test.tsx", "105ac343ba3f3224ef19ee09b2a5169ee77dc5b11a40c3ef81bf02487f3e194a"],
  ["apps/web/src/features/files/components/DatasetReleaseDialog.tsx", "243f6850414a3ec895a09fc6d9650505e5380e248546b0753024d6a78ccfa79e"],
  ["apps/web/src/features/files/components/FileTreePanel.tsx", "6fc27c1fe4c9de3dea20c37579ef968c583ee63db79db4fb59a24d2d457e6053"],
  ["apps/web/src/features/settings/components/sections/SettingsAgentCatalogSection.test.tsx", "f5610f4298398a2775e71b5ea15dcb265fd72c6793995940bf9926017d152eea"],
  ["apps/web/src/features/settings/components/sections/SettingsAgentCatalogSection.tsx", "887e1eee19cd6733d13da4a6747bb79647aeff308e762f15e5b3cb688052289d"],
  ["apps/web/src/features/settings/components/sections/SettingsSupervisorsSection.test.tsx", "e6a2e28787be230faa63e8292a8d55c3017ff56191660dff3acc95f6f590f9ed"],
  ["apps/web/src/features/settings/components/sections/SettingsSupervisorsSection.tsx", "fb59fd4acbed34554540e18e82239554aec112b2479aa197daad6b88da9e6eff"],
  ["apps/web/src/features/settings/hooks/useSettingsAgentCatalogSection.ts", "37483b38705e971b9823dc30ddf70b1b5eda694691436dcaa94de3884d78a1b6"],
  ["apps/web/src/styles/file-tree.css", "ccfb167e4a7fcd99a2951dfc37e72c3ed439276d077252c5a68d590bee197bb0"],
  ["apps/web/src/styles/settings.css", "7ea6064ccb54300c3a5ee3fc04fc2c50aae3b2f2ed3c88813d48fa7a52192fcc"],
  ["apps/web/src/styles/web-refactor.css", "8d4d0c9d2f607cd9a61effc7499d92ee7ab23a56809ed2040c20396925b0527d"],
  ["apps/web/src/styles/web.css", "133ce3b14a56d48e211950a562b864f6f830645e2045e83e88c792aa8f757c25"],
  ["apps/web/src/utils/replyCards.test.ts", "37f7835ea2b10344ea13ecf4e5f489b766d38a63ad300ebbe648d3853c6eae4c"],
  ["apps/web/src/utils/replyCards.ts", "26aa8476b601ef5fcf850e11dc3b87b10314ea25eff14a6393d98aabf5232345"],
  ["apps/web/src/utils/webThreadHistory.test.ts", "73cea50c0fb5736a41a90e09043ea0a8c696efdaacdcc4e9db6b80270633e6f5"],
]);

function listWorktreeFiles(root) {
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) files.push(...listWorktreeFiles(path));
    else files.push(path);
  }
  return files;
}

function repositoryPath(path) {
  return `apps/web/${relative(webRoot, path).replaceAll("\\", "/")}`;
}

const baselineRows = execFileSync(
  "git",
  ["ls-tree", "-r", baseline, "--", "apps/web/src"],
  { cwd: repoRoot, encoding: "utf8" },
).trim().split("\n").filter(Boolean);

const baselineFiles = new Map(
  baselineRows.map((row) => {
    const match = row.match(/^(\d+)\s+\w+\s+[0-9a-f]+\t(.+)$/);
    if (!match) throw new Error(`Unable to parse git tree entry: ${row}`);
    return [match[2], match[1]];
  }),
);
const uiOverlayFiles = new Set(
  execFileSync(
    "git",
    ["diff", "--name-only", "--no-renames", baseline, uiOverlayRef, "--", "apps/web/src"],
    { cwd: repoRoot, encoding: "utf8" },
  ).trim().split("\n").filter(Boolean),
);
for (const path of uiOverlayFiles) {
  const row = execFileSync("git", ["ls-tree", uiOverlayRef, "--", path], {
    cwd: repoRoot,
    encoding: "utf8",
  }).trim();
  const match = row.match(/^(\d+)\s+\w+\s+[0-9a-f]+\t(.+)$/);
  if (!match) {
    baselineFiles.delete(path);
    continue;
  }
  if (match[2] !== path) {
    throw new Error(`UI overlay resolved an unexpected path for ${path}`);
  }
  baselineFiles.set(path, match[1]);
}
const worktreeFiles = new Set(listWorktreeFiles(sourceRoot).map(repositoryPath));
const failures = [];

for (const [path, expectedHash] of exactReviewedFiles) {
  if (!worktreeFiles.has(path)) {
    failures.push(`${path}: reviewed file is missing from worktree`);
    continue;
  }
  const actualHash = createHash("sha256")
    .update(readFileSync(join(repoRoot, path)))
    .digest("hex");
  if (actualHash !== expectedHash) {
    failures.push(`${path}: differs from its reviewed content hash`);
  }
}

for (const [path, mode] of baselineFiles) {
  if (interfaceSeams.has(path) || exactReviewedFiles.has(path)) continue;
  if (!worktreeFiles.has(path)) {
    failures.push(`${path}: missing from worktree`);
    continue;
  }
  const absolutePath = join(repoRoot, path);
  const stat = lstatSync(absolutePath);
  const worktreeMode = stat.isSymbolicLink() ? "120000" : stat.mode & 0o111 ? "100755" : "100644";
  if (worktreeMode !== mode) {
    const expectedRef = uiOverlayFiles.has(path) ? uiOverlayRef : baseline;
    failures.push(`${path}: mode ${worktreeMode} differs from ${expectedRef} mode ${mode}`);
  }
  const expectedRef = uiOverlayFiles.has(path) ? uiOverlayRef : baseline;
  const expected = execFileSync("git", ["show", `${expectedRef}:${path}`], {
    cwd: repoRoot,
    encoding: "buffer",
  });
  const actual = readFileSync(absolutePath);
  if (!actual.equals(expected)) {
    failures.push(`${path}: content differs byte-for-byte from ${expectedRef}`);
  }
}

for (const path of worktreeFiles) {
  if (interfaceSeams.has(path) || exactReviewedFiles.has(path)) continue;
  if (!baselineFiles.has(path)) {
    failures.push(`${path}: extra file is not present in the reviewed UI snapshot`);
  }
}

if (failures.length > 0) {
  console.error(`Web UI source parity check against ${baseline} failed:`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(
  `apps/web/src UI matches ${baseline} plus ${uiOverlayFiles.size} reviewed overlay paths from ${uiOverlayRef}; `
    + `${interfaceSeams.size} adapter files and ${exactReviewedFiles.size} exact reviewed files differ.`,
);
