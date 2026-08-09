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
const reviewedDeletions = new Set([
  "apps/web/src/components/Sidebar/LearnDialog.test.tsx",
  "apps/web/src/components/Sidebar/LearnDialog.tsx",
  "apps/web/src/features/files/components/DatasetReleaseDialog.test.tsx",
  "apps/web/src/features/files/components/DatasetReleaseDialog.tsx",
  "apps/web/src/components/Sidebar/RunLauncherDialog.test.tsx",
  "apps/web/src/components/Sidebar/RunLauncherDialog.tsx",
  "apps/web/src/components/Sidebar/AgentStudioDialog.tsx",
  "apps/web/src/components/Sidebar/PythonCapabilityEditor.test.tsx",
  "apps/web/src/components/Sidebar/PythonCapabilityEditor.tsx",
  "apps/web/src/features/settings/components/sections/AgentStudioControls.tsx",
  "apps/web/src/features/settings/components/sections/SettingsAgentCatalogSection.test.tsx",
  "apps/web/src/features/settings/components/sections/SettingsAgentCatalogSection.tsx",
  "apps/web/src/features/settings/components/sections/SettingsCapabilityCatalogSection.tsx",
  "apps/web/src/features/settings/components/sections/SettingsSupervisorsSection.test.tsx",
  "apps/web/src/features/settings/components/sections/SettingsSupervisorsSection.tsx",
  "apps/web/src/features/settings/hooks/useSettingsAgentCatalogSection.ts",
  "apps/web/src/features/settings/hooks/useSettingsSupervisorsSection.ts",
]);
// The overlay is the last reviewed UI snapshot. Files intentionally changed
// after that snapshot are pinned individually so a parity exception cannot
// silently grow into unrelated presentation drift.
const exactReviewedFiles = new Map([
  ["apps/web/src/WebApp.test.tsx", "3d24c03172b4376b838e9d9efbb3470aa3520cf21d52873d577965778ec2d2d6"],
  ["apps/web/src/WebApp.tsx", "f6855bbf23dca3e11de2e621aa3abdb2bb661e51a1aec7f23b56c908cb099a87"],
  ["apps/web/src/components/Conversation/Composer.tsx", "e3954bdc54b24e7d24cd41a4e3ef4de11b1dea83ced5117f641a05b8a4c38c2b"],
  ["apps/web/src/components/Conversation/MessageList.test.tsx", "c5f3495600f8cbb147c72ef7fa19c7ef3fd7bad4bce3bba5eb2ce36e6251feaf"],
  ["apps/web/src/components/Conversation/MessageList.tsx", "6f97a2249ee3afb40bb7a7bd7c0c2d5023f633865429ff0d2f7766b284501ed8"],
  ["apps/web/src/components/Conversation/messages/AssistantMessage.test.tsx", "d2f2ff17a6a9c2187e700b0214032a0ec2b165744e563e695881752a8ca53aa9"],
  ["apps/web/src/components/Conversation/messages/AssistantMessage.tsx", "4f966a9df202742de62eb5dd0a3dc2d85bb51ca113bded409b56fcf096725845"],
  ["apps/web/src/components/Conversation/messages/ReplyCard.test.tsx", "3aa4b3ab682ccd1c86942f9c9097be1065fc04cf44cd1a89a0e632030cff9e7e"],
  ["apps/web/src/components/Conversation/messages/ReplyCard.tsx", "eda656d532c6f710cff98f396b04e0d2c32929a4615ea56db3629a278204bab6"],
  ["apps/web/src/components/Conversation/messages/ReportReplyCard.test.tsx", "17b2e0144aac3a07f2fc337a898ad946f19a15735a14d0361c6d7717abb9bff0"],
  ["apps/web/src/components/Conversation/messages/ReportReplyCard.tsx", "4127476db675bbcbeccfc1ec21b58a4a06f9b9870220f0c31e407e59dd13bcf3"],
  ["apps/web/src/components/Conversation/messages/SafeMarkdown.tsx", "1b96040f29cc4eeb4e8aeb40360569d5402ada4128f2f663b1f726566a85371c"],
  ["apps/web/src/components/Conversation/index.tsx", "ee91b929b20250f7c9301a66754d4b62b69fa15562c7d37e30cb9a5a96c05579"],
  ["apps/web/src/components/FileManager/index.tsx", "6824ba96b4edf0bf23281376a5fb4478133f6b333d4cad1476e09aeca8380491"],
  ["apps/web/src/components/FileManager/index.test.tsx", "ea2dcc0097f939246a9596190e1a0070bd07e4025b6008cbeefbf58b9a1f9c58"],
  ["apps/web/src/components/Sidebar/McpStatus.test.tsx", "98f6eb2a9d2a3bf87810d2b6131d830b6721374c1cbc1c0649981ab97ec991cd"],
  ["apps/web/src/components/Sidebar/McpStatus.tsx", "ecddc1f687e06ea0394354245f3d3c29fa08e8f6e793550866f48a0d1f8e75c7"],
  ["apps/web/src/components/Sidebar/Workspaces.test.tsx", "88fe19f3bbf6c88e6e67f292ae538e082b80932ec2f0446d9f98770c61ab1811"],
  ["apps/web/src/components/Sidebar/Workspaces.tsx", "13e3ea2351de56e71934c16f36b485ba2c6609ecbe7c6afd155e013c1b407a9e"],
  ["apps/web/src/components/Sidebar/index.test.tsx", "a153ccdbc098c6ec68730e6ac8c3cb2b526071bf1fbbed041bc7d03cc4499729"],
  ["apps/web/src/components/Sidebar/index.tsx", "2bfd4cba6b426d8aefb0cb6208c5ce1de82752a1b58488163311f8dba1aeebfb"],
  ["apps/web/src/features/app/hooks/useSettingsModalState.ts", "c2d8ca76029f423e1d442682d5e6a7c6f80eb2e1abd4f07cc825ab32e05bd99a"],
  ["apps/web/src/features/app/hooks/useMainAppLayoutSurfaces.ts", "48b7a323ca22049108912d5f9cda520f24ae9787c7bdb02f7403bf6b6cd23414"],
  ["apps/web/src/features/files/components/FileTreePanel.tsx", "6fc27c1fe4c9de3dea20c37579ef968c583ee63db79db4fb59a24d2d457e6053"],
  ["apps/web/src/features/settings/components/SettingsNav.tsx", "3543b1c8aa3480ce0f47f4adff346ba543be19a04d88456150deb2bd4e41cdab"],
  ["apps/web/src/features/settings/components/sections/SettingsSectionContainers.tsx", "69ef28bf42c648c1244b84837780724863dc06c2248daf79284c2b0bf2249e10"],
  ["apps/web/src/features/settings/components/settingsTypes.ts", "76cc2a423f70f499067e2fb5fae2546eb489faca5ed977c0f1897d3b76ecfca3"],
  ["apps/web/src/features/settings/components/settingsViewConstants.ts", "0f1db75484bba8e650fbd4b8e1d466af38bc9c01f942999f0005b64aac79c134"],
  ["apps/web/src/features/settings/hooks/useSettingsViewOrchestration.ts", "0f7fe4507ad33951a71a063e5316888db09b9433112364e394ac1843bf472886"],
  ["apps/web/src/styles/file-tree.css", "41074cb28f684568b254f9f1a110a84e4cbbc0629e6fe772f07c310388a5bfd0"],
  ["apps/web/src/styles/settings.css", "fd2965f57f4442088e07f83ad99bf15181e0775a4899404c3e7f8fd2d4c8ef73"],
  ["apps/web/src/styles/web-refactor.css", "b7d9e88f778434a1f907281665c1ccbe30c5611a082c7d4bbf99dfbe7695002c"],
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
  if (interfaceSeams.has(path) || exactReviewedFiles.has(path) || reviewedDeletions.has(path)) {
    continue;
  }
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
  if (interfaceSeams.has(path) || exactReviewedFiles.has(path) || reviewedDeletions.has(path)) {
    continue;
  }
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
    + `${interfaceSeams.size} adapter files, ${exactReviewedFiles.size} exact reviewed files, and `
    + `${reviewedDeletions.size} reviewed deletions differ.`,
);
