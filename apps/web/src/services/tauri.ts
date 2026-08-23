import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Options as NotificationOptions } from "@tauri-apps/plugin-notification";
import type {
  AppSettings,
  CodexUpdateResult,
  CodexDoctorResult,
  DictationModelStatus,
  DictationSessionState,
  LocalUsageSnapshot,
  TcpDaemonStatus,
  TailscaleDaemonCommandPreview,
  TailscaleStatus,
  TrayRecentThreadEntry,
  TraySessionUsage,
  WorkspaceInfo,
  AppMention,
} from "../types";
import type {
  GitFileDiff,
  GitFileStatus,
  GitCommitDiff,
  GitHubIssuesResponse,
  GitHubPullRequestComment,
  GitHubPullRequestDiff,
  GitHubPullRequestsResponse,
  GitLogResponse,
  ReviewTarget,
} from "../types";

function isMissingTauriInvokeError(error: unknown) {
  return (
    error instanceof TypeError &&
    (error.message.includes("reading 'invoke'") ||
      error.message.includes("reading \"invoke\""))
  );
}

export async function pickWorkspacePath(): Promise<string | null> {
  const selection = await open({ directory: true, multiple: false });
  if (!selection || Array.isArray(selection)) {
    return null;
  }
  return selection;
}

export async function pickWorkspacePaths(): Promise<string[]> {
  const selection = await open({ directory: true, multiple: true });
  if (!selection) {
    return [];
  }
  return Array.isArray(selection) ? selection : [selection];
}

export async function pickImageFiles(): Promise<string[]> {
  const selection = await open({
    multiple: true,
    filters: [
      {
        name: "Images",
        extensions: [
          "png",
          "jpg",
          "jpeg",
          "gif",
          "webp",
          "bmp",
          "tiff",
          "tif",
          "heic",
          "heif",
        ],
      },
    ],
  });
  if (!selection) {
    return [];
  }
  return Array.isArray(selection) ? selection : [selection];
}

export async function exportMarkdownFile(
  content: string,
  defaultFileName = "plan.md",
): Promise<string | null> {
  const selection = await save({
    title: "Export plan as Markdown",
    defaultPath: defaultFileName,
    filters: [
      {
        name: "Markdown",
        extensions: ["md"],
      },
    ],
  });
  if (!selection) {
    return null;
  }
  await invoke("write_text_file", { path: selection, content });
  return selection;
}

export async function listWorkspaces(): Promise<WorkspaceInfo[]> {
  try {
    return await invoke<WorkspaceInfo[]>("list_workspaces");
  } catch (error) {
    if (isMissingTauriInvokeError(error)) {
      // In non-Tauri environments (e.g., Electron/web previews), the invoke
      // bridge may be missing. Treat this as "no workspaces" instead of crashing.
      console.warn("Tauri invoke bridge unavailable; returning empty workspaces list.");
      return [];
    }
    throw error;
  }
}

export async function getCodexConfigPath(): Promise<string> {
  return invoke<string>("get_codex_config_path");
}

export type WorkspaceAgentsFileResponse = {
  exists: boolean;
  content: string;
  truncated: boolean;
};

export async function readImageAsDataUrl(path: string): Promise<string> {
  return invoke<string>("read_image_as_data_url", { path });
}

export async function addWorkspace(path: string): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("add_workspace", { path });
}

export async function addWorkspaceFromGitUrl(
  url: string,
  destinationPath: string,
  targetFolderName: string | null,
): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("add_workspace_from_git_url", {
    url,
    destinationPath,
    targetFolderName,
  });
}

export async function isWorkspacePathDir(path: string): Promise<boolean> {
  return invoke<boolean>("is_workspace_path_dir", { path });
}

export async function addClone(
  sourceWorkspaceId: string,
  copiesFolder: string,
  copyName: string,
): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("add_clone", {
    sourceWorkspaceId,
    copiesFolder,
    copyName,
  });
}

export async function addWorktree(
  parentId: string,
  branch: string,
  name: string | null,
  copyAgentsMd = true,
): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("add_worktree", { parentId, branch, name, copyAgentsMd });
}

export async function removeWorkspace(id: string): Promise<void> {
  return invoke("remove_workspace", { id });
}

export async function removeWorktree(id: string): Promise<void> {
  return invoke("remove_worktree", { id });
}

export async function renameWorktree(
  id: string,
  branch: string,
): Promise<WorkspaceInfo> {
  return invoke<WorkspaceInfo>("rename_worktree", { id, branch });
}

export async function renameWorktreeUpstream(
  id: string,
  oldBranch: string,
  newBranch: string,
): Promise<void> {
  return invoke("rename_worktree_upstream", { id, oldBranch, newBranch });
}

export async function applyWorktreeChanges(workspaceId: string): Promise<void> {
  return invoke("apply_worktree_changes", { workspaceId });
}

export async function openWorkspaceIn(
  path: string,
  options: {
    appName?: string | null;
    command?: string | null;
    args?: string[];
    line?: number | null;
    column?: number | null;
  },
): Promise<void> {
  return invoke("open_workspace_in", {
    path,
    app: options.appName ?? null,
    command: options.command ?? null,
    args: options.args ?? [],
    line: options.line ?? null,
    column: options.column ?? null,
  });
}

export async function getOpenAppIcon(appName: string): Promise<string | null> {
  return invoke<string | null>("get_open_app_icon", { appName });
}

export async function connectWorkspace(id: string): Promise<void> {
  return invoke("connect_workspace", { id });
}

export async function startThread(workspaceId: string) {
  return invoke<any>("start_thread", { workspaceId });
}

export async function forkThread(workspaceId: string, threadId: string) {
  return invoke<any>("fork_thread", { workspaceId, threadId });
}

export async function compactThread(workspaceId: string, threadId: string) {
  return invoke<any>("compact_thread", { workspaceId, threadId });
}

function isInlineImageUrl(image: string) {
  return (
    image.startsWith("data:") ||
    image.startsWith("http://") ||
    image.startsWith("https://")
  );
}

async function convertImagesToDataUrls(images: string[]): Promise<string[]> {
  return Promise.all(
    images.map(async (image) => {
      if (isInlineImageUrl(image)) {
        return image;
      }
      return readImageAsDataUrl(image);
    }),
  );
}

async function normalizeImagesForRpc(images?: string[]): Promise<string[] | null> {
  if (images == null) {
    return null;
  }
  if (images.length === 0) {
    return [];
  }
  const hasPathImages = images.some((image) => !isInlineImageUrl(image));
  if (!hasPathImages) {
    return images;
  }
  let settings: AppSettings;
  let mobileRuntime: boolean;
  try {
    [settings, mobileRuntime] = await Promise.all([getAppSettings(), isMobileRuntime()]);
  } catch (error) {
    if (isMissingTauriInvokeError(error)) {
      return images;
    }
    throw error;
  }
  if (settings.backendMode !== "remote" && !mobileRuntime) {
    return images;
  }
  return convertImagesToDataUrls(images);
}

export async function sendUserMessage(
  workspaceId: string,
  threadId: string,
  text: string,
  options?: {
    model?: string | null;
    effort?: string | null;
    serviceTier?: "fast" | "flex" | null | undefined;
    accessMode?: "read-only" | "current" | "full-access";
    images?: string[];
    collaborationMode?: Record<string, unknown> | null;
    appMentions?: AppMention[];
  },
) {
  const images = await normalizeImagesForRpc(options?.images);
  const payload: Record<string, unknown> = {
    workspaceId,
    threadId,
    text,
    model: options?.model ?? null,
    effort: options?.effort ?? null,
    accessMode: options?.accessMode ?? null,
    images,
  };
  if (options?.serviceTier !== undefined) {
    payload.serviceTier = options.serviceTier;
  }
  if (options?.collaborationMode) {
    payload.collaborationMode = options.collaborationMode;
  }
  if (options?.appMentions && options.appMentions.length > 0) {
    payload.appMentions = options.appMentions;
  }
  return invoke("send_user_message", payload);
}

export async function interruptTurn(
  workspaceId: string,
  threadId: string,
  turnId: string,
) {
  return invoke("turn_interrupt", { workspaceId, threadId, turnId });
}

export async function steerTurn(
  workspaceId: string,
  threadId: string,
  turnId: string,
  text: string,
  images?: string[],
  appMentions?: AppMention[],
) {
  const normalizedImages = await normalizeImagesForRpc(images);
  const payload: Record<string, unknown> = {
    workspaceId,
    threadId,
    turnId,
    text,
    images: normalizedImages,
  };
  if (appMentions && appMentions.length > 0) {
    payload.appMentions = appMentions;
  }
  return invoke("turn_steer", payload);
}

export async function startReview(
  workspaceId: string,
  threadId: string,
  target: ReviewTarget,
  delivery?: "inline" | "detached",
) {
  const payload: Record<string, unknown> = { workspaceId, threadId, target };
  if (delivery) {
    payload.delivery = delivery;
  }
  return invoke("start_review", payload);
}

export async function respondToServerRequest(
  workspaceId: string,
  requestId: number | string,
  decision: "accept" | "decline",
) {
  return invoke("respond_to_server_request", {
    workspaceId,
    requestId,
    result: { decision },
  });
}

export async function respondToUserInputRequest(
  workspaceId: string,
  requestId: number | string,
  answers: Record<string, { answers: string[] }>,
) {
  return invoke("respond_to_server_request", {
    workspaceId,
    requestId,
    result: { answers },
  });
}

export async function getGitStatus(workspace_id: string): Promise<{
  branchName: string;
  files: GitFileStatus[];
  stagedFiles: GitFileStatus[];
  unstagedFiles: GitFileStatus[];
  totalAdditions: number;
  totalDeletions: number;
}> {
  return invoke("get_git_status", { workspaceId: workspace_id });
}

export type InitGitRepoResponse =
  | { status: "initialized"; commitError?: string }
  | { status: "already_initialized" }
  | { status: "needs_confirmation"; entryCount: number };

export async function initGitRepo(
  workspaceId: string,
  branch: string,
  force = false,
): Promise<InitGitRepoResponse> {
  return invoke<InitGitRepoResponse>("init_git_repo", { workspaceId, branch, force });
}

export type CreateGitHubRepoResponse =
  | { status: "ok"; repo: string; remoteUrl?: string | null }
  | {
      status: "partial";
      repo: string;
      remoteUrl?: string | null;
      pushError?: string | null;
      defaultBranchError?: string | null;
    };

export async function createGitHubRepo(
  workspaceId: string,
  repo: string,
  visibility: "private" | "public",
  branch?: string | null,
): Promise<CreateGitHubRepoResponse> {
  return invoke<CreateGitHubRepoResponse>("create_github_repo", {
    workspaceId,
    repo,
    visibility,
    branch,
  });
}

export async function getGitDiffs(
  workspace_id: string,
): Promise<GitFileDiff[]> {
  return invoke("get_git_diffs", { workspaceId: workspace_id });
}

export async function getGitLog(
  workspace_id: string,
  limit = 40,
): Promise<GitLogResponse> {
  return invoke("get_git_log", { workspaceId: workspace_id, limit });
}

export async function getGitCommitDiff(
  workspace_id: string,
  sha: string,
): Promise<GitCommitDiff[]> {
  return invoke("get_git_commit_diff", { workspaceId: workspace_id, sha });
}

export async function getGitRemote(workspace_id: string): Promise<string | null> {
  return invoke("get_git_remote", { workspaceId: workspace_id });
}

export async function stageGitFile(workspaceId: string, path: string) {
  return invoke("stage_git_file", { workspaceId, path });
}

export async function stageGitAll(workspaceId: string): Promise<void> {
  return invoke("stage_git_all", { workspaceId });
}

export async function unstageGitFile(workspaceId: string, path: string) {
  return invoke("unstage_git_file", { workspaceId, path });
}

export async function revertGitFile(workspaceId: string, path: string) {
  return invoke("revert_git_file", { workspaceId, path });
}

export async function revertGitAll(workspaceId: string) {
  return invoke("revert_git_all", { workspaceId });
}

export async function commitGit(
  workspaceId: string,
  message: string,
): Promise<void> {
  return invoke("commit_git", { workspaceId, message });
}

export async function pushGit(workspaceId: string): Promise<void> {
  return invoke("push_git", { workspaceId });
}

export async function pullGit(workspaceId: string): Promise<void> {
  return invoke("pull_git", { workspaceId });
}

export async function fetchGit(workspaceId: string): Promise<void> {
  return invoke("fetch_git", { workspaceId });
}

export async function syncGit(workspaceId: string): Promise<void> {
  return invoke("sync_git", { workspaceId });
}

export async function getGitHubIssues(
  workspace_id: string,
): Promise<GitHubIssuesResponse> {
  return invoke("get_github_issues", { workspaceId: workspace_id });
}

export async function getGitHubPullRequests(
  workspace_id: string,
): Promise<GitHubPullRequestsResponse> {
  return invoke("get_github_pull_requests", { workspaceId: workspace_id });
}

export async function getGitHubPullRequestDiff(
  workspace_id: string,
  prNumber: number,
): Promise<GitHubPullRequestDiff[]> {
  return invoke("get_github_pull_request_diff", {
    workspaceId: workspace_id,
    prNumber,
  });
}

export async function getGitHubPullRequestComments(
  workspace_id: string,
  prNumber: number,
): Promise<GitHubPullRequestComment[]> {
  return invoke("get_github_pull_request_comments", {
    workspaceId: workspace_id,
    prNumber,
  });
}

export async function checkoutGitHubPullRequest(
  workspace_id: string,
  prNumber: number,
): Promise<void> {
  return invoke("checkout_github_pull_request", {
    workspaceId: workspace_id,
    prNumber,
  });
}

export async function localUsageSnapshot(
  days?: number,
  workspacePath?: string | null,
): Promise<LocalUsageSnapshot> {
  const payload: { days: number; workspacePath?: string } = { days: days ?? 30 };
  if (workspacePath) {
    payload.workspacePath = workspacePath;
  }
  return invoke("local_usage_snapshot", payload);
}

export async function getModelList(workspaceId: string) {
  return invoke<any>("model_list", { workspaceId });
}

export async function getModelProviderList(workspaceId: string) {
  return invoke<any>("model_provider_list", { workspaceId });
}

export async function writeModelProvider(workspaceId: string, input: Record<string, unknown>) {
  return invoke<any>("model_provider_write", { workspaceId, input });
}

export async function getExperimentalFeatureList(
  workspaceId: string,
  cursor?: string | null,
  limit?: number | null,
) {
  return invoke<any>("experimental_feature_list", { workspaceId, cursor, limit });
}

export async function setCodexFeatureFlag(
  featureKey: string,
  enabled: boolean,
): Promise<void> {
  return invoke("set_codex_feature_flag", { featureKey, enabled });
}

export async function getCollaborationModes(workspaceId: string) {
  return invoke<any>("collaboration_mode_list", { workspaceId });
}

export async function getAccountRateLimits(workspaceId: string) {
  return invoke<any>("account_rate_limits", { workspaceId });
}

export async function getAccountInfo(workspaceId: string) {
  return invoke<any>("account_read", { workspaceId });
}

export async function runCodexLogin(workspaceId: string) {
  return invoke<{ loginId: string; authUrl: string; raw?: unknown }>("codex_login", {
    workspaceId,
  });
}

export async function cancelCodexLogin(workspaceId: string) {
  return invoke<{ canceled: boolean; status?: string; raw?: unknown }>(
    "codex_login_cancel",
    { workspaceId },
  );
}

export async function getSkillsList(workspaceId: string) {
  return invoke<any>("skills_list", { workspaceId });
}

export async function getAppsList(
  workspaceId: string,
  cursor?: string | null,
  limit?: number | null,
  threadId?: string | null,
) {
  return invoke<any>("apps_list", { workspaceId, cursor, limit, threadId });
}

export async function getAppSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_app_settings");
}

export async function isMobileRuntime(): Promise<boolean> {
  return invoke<boolean>("is_mobile_runtime");
}

export async function updateAppSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_app_settings", { settings });
}

export async function tailscaleStatus(): Promise<TailscaleStatus> {
  return invoke<TailscaleStatus>("tailscale_status");
}

export async function tailscaleDaemonCommandPreview(): Promise<TailscaleDaemonCommandPreview> {
  return invoke<TailscaleDaemonCommandPreview>("tailscale_daemon_command_preview");
}

export async function tailscaleDaemonStart(): Promise<TcpDaemonStatus> {
  return invoke<TcpDaemonStatus>("tailscale_daemon_start");
}

export async function tailscaleDaemonStop(): Promise<TcpDaemonStatus> {
  return invoke<TcpDaemonStatus>("tailscale_daemon_stop");
}

export async function tailscaleDaemonStatus(): Promise<TcpDaemonStatus> {
  return invoke<TcpDaemonStatus>("tailscale_daemon_status");
}

type MenuAcceleratorUpdate = {
  id: string;
  accelerator: string | null;
};

export async function setMenuAccelerators(
  updates: MenuAcceleratorUpdate[],
): Promise<void> {
  return invoke("menu_set_accelerators", { updates });
}

export async function runCodexDoctor(
  codexBin: string | null,
  codexArgs: string | null,
): Promise<CodexDoctorResult> {
  return invoke<CodexDoctorResult>("codex_doctor", { codexBin, codexArgs });
}

export async function runCodexUpdate(
  codexBin: string | null,
  codexArgs: string | null,
): Promise<CodexUpdateResult> {
  return invoke<CodexUpdateResult>("codex_update", { codexBin, codexArgs });
}

export async function getWorkspaceFiles(workspaceId: string) {
  return invoke<string[]>("list_workspace_files", { workspaceId });
}

export async function readWorkspaceFile(
  workspaceId: string,
  path: string,
): Promise<{ content: string; truncated: boolean }> {
  return invoke<{ content: string; truncated: boolean }>("read_workspace_file", {
    workspaceId,
    path,
  });
}

export async function readAgentMd(
  workspaceId: string,
): Promise<WorkspaceAgentsFileResponse> {
  return invoke<WorkspaceAgentsFileResponse>("file_read", {
    scope: "workspace",
    kind: "agents",
    workspaceId,
  });
}

export async function writeAgentMd(workspaceId: string, content: string): Promise<void> {
  return invoke("file_write", {
    scope: "workspace",
    kind: "agents",
    workspaceId,
    content,
  });
}

export async function listGitBranches(workspaceId: string) {
  return invoke<any>("list_git_branches", { workspaceId });
}

export async function checkoutGitBranch(workspaceId: string, name: string) {
  return invoke("checkout_git_branch", { workspaceId, name });
}

export async function createGitBranch(workspaceId: string, name: string) {
  return invoke("create_git_branch", { workspaceId, name });
}

function withModelId(modelId?: string | null) {
  return modelId ? { modelId } : {};
}

export async function getDictationModelStatus(
  modelId?: string | null,
): Promise<DictationModelStatus> {
  return invoke<DictationModelStatus>(
    "dictation_model_status",
    withModelId(modelId),
  );
}

export async function downloadDictationModel(
  modelId?: string | null,
): Promise<DictationModelStatus> {
  return invoke<DictationModelStatus>(
    "dictation_download_model",
    withModelId(modelId),
  );
}

export async function cancelDictationDownload(
  modelId?: string | null,
): Promise<DictationModelStatus> {
  return invoke<DictationModelStatus>(
    "dictation_cancel_download",
    withModelId(modelId),
  );
}

export async function removeDictationModel(
  modelId?: string | null,
): Promise<DictationModelStatus> {
  return invoke<DictationModelStatus>(
    "dictation_remove_model",
    withModelId(modelId),
  );
}

export async function startDictation(
  preferredLanguage: string | null,
): Promise<DictationSessionState> {
  return invoke("dictation_start", { preferredLanguage });
}

export async function requestDictationPermission(): Promise<boolean> {
  return invoke("dictation_request_permission");
}

export async function stopDictation(): Promise<DictationSessionState> {
  return invoke("dictation_stop");
}

export async function cancelDictation(): Promise<DictationSessionState> {
  return invoke("dictation_cancel");
}

export async function listThreads(
  workspaceId: string,
  cursor?: string | null,
  limit?: number | null,
  sortKey?: "created_at" | "updated_at" | null,
) {
  return invoke<any>("list_threads", { workspaceId, cursor, limit, sortKey });
}

export async function listMcpServerStatus(
  workspaceId: string,
  cursor?: string | null,
  limit?: number | null,
) {
  return invoke<any>("list_mcp_server_status", { workspaceId, cursor, limit });
}

export async function resumeThread(workspaceId: string, threadId: string) {
  return invoke<any>("resume_thread", { workspaceId, threadId });
}

export async function readThread(workspaceId: string, threadId: string) {
  return invoke<any>("read_thread", { workspaceId, threadId });
}

export async function listThreadTurns(
  workspaceId: string,
  threadId: string,
  cursor?: string | null,
) {
  return invoke<any>("list_thread_turns", { workspaceId, threadId, cursor });
}

export async function threadLiveSubscribe(workspaceId: string, threadId: string) {
  return invoke<any>("thread_live_subscribe", { workspaceId, threadId });
}

export async function threadLiveUnsubscribe(workspaceId: string, threadId: string) {
  return invoke<any>("thread_live_unsubscribe", { workspaceId, threadId });
}

export async function archiveThread(workspaceId: string, threadId: string) {
  return invoke<any>("archive_thread", { workspaceId, threadId });
}

export async function setThreadName(
  workspaceId: string,
  threadId: string,
  name: string,
) {
  return invoke<any>("set_thread_name", { workspaceId, threadId, name });
}

export async function setTrayRecentThreads(entries: TrayRecentThreadEntry[]) {
  return invoke<void>("set_tray_recent_threads", { entries });
}

export async function setTraySessionUsage(usage: TraySessionUsage | null) {
  return invoke<void>("set_tray_session_usage", { usage });
}

export type AppBuildType = "debug" | "release";

export async function getAppBuildType(): Promise<AppBuildType> {
  return invoke<AppBuildType>("app_build_type");
}

export async function sendNotification(
  title: string,
  body: string,
  options?: {
    id?: number;
    group?: string;
    actionTypeId?: string;
    sound?: string;
    autoCancel?: boolean;
    extra?: Record<string, unknown>;
  },
): Promise<void> {
  const macosDebugBuild = await invoke<boolean>("is_macos_debug_build").catch(
    () => false,
  );
  const attemptFallback = async () => {
    try {
      await invoke("send_notification_fallback", { title, body });
      return true;
    } catch (error) {
      console.warn("Notification fallback failed.", { error });
      return false;
    }
  };

  // In dev builds on macOS, the notification plugin can silently fail because
  // the process is not a bundled app. Prefer the native AppleScript fallback.
  if (macosDebugBuild) {
    await attemptFallback();
    return;
  }

  try {
    const notification = await import("@tauri-apps/plugin-notification");
    let permissionGranted = await notification.isPermissionGranted();
    if (!permissionGranted) {
      const permission = await notification.requestPermission();
      permissionGranted = permission === "granted";
      if (!permissionGranted) {
        console.warn("Notification permission not granted.", { permission });
        await attemptFallback();
        return;
      }
    }
    if (permissionGranted) {
      const payload: NotificationOptions = { title, body };
      if (options?.id !== undefined) {
        payload.id = options.id;
      }
      if (options?.group !== undefined) {
        payload.group = options.group;
      }
      if (options?.actionTypeId !== undefined) {
        payload.actionTypeId = options.actionTypeId;
      }
      if (options?.sound !== undefined) {
        payload.sound = options.sound;
      }
      if (options?.autoCancel !== undefined) {
        payload.autoCancel = options.autoCancel;
      }
      if (options?.extra !== undefined) {
        payload.extra = options.extra;
      }
      await notification.sendNotification(payload);
      return;
    }
  } catch (error) {
    console.warn("Notification plugin failed.", { error });
  }

  await attemptFallback();
}
