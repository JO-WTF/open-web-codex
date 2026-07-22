import type { Project, Run, RunEvent, Task } from "../types";
import type { AppServerEvent, AppSettings, WorkspaceInfo, WorkspaceSettings } from "@/types";
import { loadAppSettings, saveAppSettings } from "../defaultSettings";
import { platformClient } from "../session";
import { registerBrowserCommand } from "./core";
import { emitBrowserEvent } from "./event";
import { openUrl } from "./opener";
import {
  browserDictationModelStatus,
  cancelBrowserDictation,
  requestBrowserDictationPermission,
  startBrowserDictation,
  stopBrowserDictation,
} from "./dictation";

type Payload = Record<string, unknown>;
type ThreadContext = { projectId: string; taskId: string; runId: string };

const threadContexts = new Map<string, ThreadContext>();
const workspaceRuns = new Map<string, ThreadContext>();
const selectedWorkspaceRuns = new Map<string, ThreadContext>();
const workspaceSettings = new Map<string, WorkspaceSettings>();
const profileLoginWatches = new Map<string, { loginId: string; stopped: boolean }>();
let unsubscribeEvents: (() => void) | null = null;

function requiredString(payload: Payload, key: string): string {
  const value = payload[key];
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`${key} is required`);
  }
  return value.trim();
}

function projectPath(project: Project) {
  return project.git_url || `project:${project.id}`;
}

function settingsFor(projectId: string): WorkspaceSettings {
  return workspaceSettings.get(projectId) ?? { sidebarCollapsed: false };
}

function projectWorkspace(project: Project): WorkspaceInfo {
  return {
    id: project.id,
    name: project.name,
    path: projectPath(project),
    connected: true,
    kind: "main",
    settings: settingsFor(project.id),
  };
}

function runWorkspace(project: Project, task: Task, run: Run): WorkspaceInfo {
  const branch = run.source_ref ?? project.default_branch;
  const context = { projectId: project.id, taskId: task.id, runId: run.id };
  workspaceRuns.set(run.id, context);
  if (run.codex_thread_id) threadContexts.set(run.codex_thread_id, context);
  return {
    id: run.id,
    name: run.workspace_name ?? branch ?? task.title,
    path: `workspace:${run.id}`,
    connected: Boolean(run.workspace_id && run.codex_thread_id && run.status !== "failed"),
    kind: "worktree",
    parentId: project.id,
    worktree: { branch },
    settings: settingsFor(run.id),
  };
}

async function runsForTasks(tasks: Task[]) {
  return await Promise.all(tasks.map(async (task) => ({
    task,
    runs: await platformClient.listRuns(task.id),
  })));
}

async function indexAllProjectThreads(projectId: string) {
  const project = await platformClient.getProject(projectId);
  const rows = await runsForTasks(await platformClient.listTasks(projectId));
  const indexed = rows.flatMap(({ task, runs }) => {
    return runs.flatMap((run) => {
      const threadId = run.codex_thread_id;
      if (!threadId) return [];
      const context = { projectId, taskId: task.id, runId: run.id };
      threadContexts.set(threadId, context);
      return [{ project, task, run, threadId, context }];
    });
  });
  return indexed;
}

async function indexProjectThreads(projectId: string) {
  return (await indexAllProjectThreads(projectId)).filter(({ run }) => run.workspace_kind === "main");
}

async function listBrowserWorkspaces() {
  workspaceRuns.clear();
  workspaceSettings.clear();
  for (const preference of await platformClient.listBrowserWorkspacePreferences()) {
    workspaceSettings.set(preference.workspaceId, preference.settings as WorkspaceSettings);
  }
  const projects = await platformClient.listProjects();
  const entries = await Promise.all(projects.map(async (project) => {
    const rows = await runsForTasks(await platformClient.listTasks(project.id));
    const children = rows.flatMap(({ task, runs }) => runs
      .filter((run) => run.workspace_kind !== "main" && !["cancelled", "failed"].includes(run.status))
      .filter((run) => !run.workspace_group_run_id)
      .map((run) => runWorkspace(project, task, run)));
    return [projectWorkspace(project), ...children];
  }));
  return entries.flat();
}

async function findWorkspaceRun(workspaceId: string) {
  const cached = workspaceRuns.get(workspaceId);
  if (cached) return cached;
  await listBrowserWorkspaces();
  return workspaceRuns.get(workspaceId) ?? null;
}

async function projectContextForWorkspace(workspaceId: string) {
  const run = await findWorkspaceRun(workspaceId);
  if (run) {
    return { project: await platformClient.getProject(run.projectId), run };
  }
  return { project: await platformClient.getProject(workspaceId), run: null };
}

async function createDerivedWorkspace(
  sourceWorkspaceId: string,
  branch: string,
  name: string | null,
  kind: "worktree" | "clone",
  copyAgentsMd = false,
) {
  const { project, run: sourceContext } = await projectContextForWorkspace(sourceWorkspaceId);
  let parentRun = sourceContext
    ? await platformClient.getRun(sourceContext.runId)
    : await runForWorkspace(project.id).catch(() => null);
  if (!parentRun) {
    const parentTask = await platformClient.createTask(project.id, "New Agent");
    const started = await platformClient.startRun(parentTask.id);
    parentRun = await waitForThread(project.id, parentTask.id, started.run.id);
  }
  const title = name?.trim() || branch;
  const task = await platformClient.createTask(project.id, title);
  const { run } = await platformClient.startRun(task.id, branch, {
    kind,
    name: title,
    parentRunId: parentRun?.id ?? null,
    copyAgentsMd,
  });
  const ready = await waitForThread(project.id, task.id, run.id);
  const inheritedSettings = settingsFor(sourceWorkspaceId);
  await platformClient.updateBrowserWorkspaceSettings(ready.id, inheritedSettings as Record<string, unknown>);
  workspaceSettings.set(ready.id, inheritedSettings);
  return runWorkspace(project, task, ready);
}

async function createThreadRunInWorkspace(workspaceId: string, forkThreadId?: string | null) {
  const derivedRoot = await findWorkspaceRun(workspaceId);
  const projectId = derivedRoot?.projectId ?? workspaceId;
  const project = await platformClient.getProject(projectId);
  const rootRun = derivedRoot ? await platformClient.getRun(derivedRoot.runId) : null;
  const sourceContext = forkThreadId ? await findThreadContext(forkThreadId) : null;
  const sourceRun = sourceContext ? await platformClient.getRun(sourceContext.runId) : rootRun;
  if (sourceContext && sourceContext.projectId !== projectId) {
    throw new Error("Fork source Thread is outside the selected project");
  }
  if (rootRun && sourceRun) {
    const sourceRootId = sourceRun.workspace_group_run_id ?? sourceRun.id;
    if (sourceRootId !== rootRun.id) {
      throw new Error("Fork source Thread is outside the selected worktree");
    }
  }
  const sourceTask = sourceContext ? await platformClient.getTask(sourceContext.taskId) : null;
  const task = await platformClient.createTask(
    projectId,
    sourceTask ? `${sourceTask.title} (Fork)` : "New Agent",
  );
  const { run } = await platformClient.startRun(task.id, sourceRun?.source_ref ?? undefined, {
    kind: rootRun?.workspace_kind ?? "main",
    name: rootRun?.workspace_name ?? null,
    parentRunId: rootRun?.workspace_parent_run_id ?? null,
    groupRunId: rootRun?.id ?? null,
    forkThreadId: forkThreadId ?? null,
    forkSourceRunId: forkThreadId ? sourceRun?.id ?? null : null,
  });
  const ready = await waitForThread(projectId, task.id, run.id);
  const context = { projectId, taskId: task.id, runId: ready.id };
  if (ready.codex_thread_id) threadContexts.set(ready.codex_thread_id, context);
  selectedWorkspaceRuns.set(workspaceId, context);
  return { project, task, run: ready, context };
}

async function rawRunForWorkspace(projectId: string) {
  const selected = selectedWorkspaceRuns.get(projectId);
  if (selected) return await platformClient.getRun(selected.runId);
  const direct = await findWorkspaceRun(projectId);
  if (direct) return await platformClient.getRun(direct.runId);
  const indexed = await indexProjectThreads(projectId);
  const selectedRun = indexed.find(({ run }) => run.active_turn_id || run.status === "running")
    ?? indexed[0];
  if (!selectedRun) {
    throw new Error("This project does not have a ready Run workspace yet");
  }
  return selectedRun.run;
}

async function runForWorkspace(projectId: string) {
  return await configureRunGitRoot(projectId, await rawRunForWorkspace(projectId));
}

async function configureRunGitRoot(browserWorkspaceId: string, run: Run) {
  const selected = settingsFor(browserWorkspaceId).gitRoot?.trim() || null;
  await platformClient.setWorkspaceGitRoot(run.id, selected);
  return run;
}

function stopProfileLoginWatch(workspaceId: string) {
  const watch = profileLoginWatches.get(workspaceId);
  if (watch) watch.stopped = true;
  profileLoginWatches.delete(workspaceId);
}

async function watchProfileLogin(workspaceId: string, loginId: string) {
  stopProfileLoginWatch(workspaceId);
  const watch = { loginId, stopped: false };
  profileLoginWatches.set(workspaceId, watch);
  const startedAt = Date.now();

  while (!watch.stopped && profileLoginWatches.get(workspaceId) === watch) {
    try {
      const status = await platformClient.profileLoginStatus(loginId);
      if (status.completed) {
        profileLoginWatches.delete(workspaceId);
        emitBrowserEvent<AppServerEvent>("app-server-event", {
          workspace_id: workspaceId,
          message: {
            method: "account/login/completed",
            params: {
              loginId,
              success: status.success === true,
              error: status.error,
            },
          },
        });
        return;
      }
    } catch {
      // A transient Profile Host restart is retried within the login window.
    }

    if (Date.now() - startedAt >= 5 * 60_000) {
      profileLoginWatches.delete(workspaceId);
      emitBrowserEvent<AppServerEvent>("app-server-event", {
        workspace_id: workspaceId,
        message: {
          method: "account/login/completed",
          params: { loginId, success: false, error: "Codex login timed out." },
        },
      });
      return;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 1_000));
  }
}

async function findThreadContext(threadId: string): Promise<ThreadContext> {
  const cached = threadContexts.get(threadId);
  if (cached) return cached;
  for (const project of await platformClient.listProjects()) {
    const found = (await indexAllProjectThreads(project.id)).find((entry) => entry.threadId === threadId);
    if (found) return found.context;
  }
  throw new Error("Thread is not available in an authorized project");
}

async function waitForThread(projectId: string, taskId: string, runId: string) {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const run = await platformClient.getRun(runId);
    if (run.codex_thread_id) {
      threadContexts.set(run.codex_thread_id, { projectId, taskId, runId });
      return run;
    }
    if (["failed", "cancelled"].includes(run.status)) {
      throw new Error(`Run ${run.status} before its Codex Thread was ready`);
    }
    await new Promise((resolve) => window.setTimeout(resolve, 200));
  }
  throw new Error("Timed out waiting for the Codex Thread to become ready");
}

function rawItem(event: RunEvent) {
  const itemId = event.item_id;
  const itemType = event.payload.itemType;
  if (!itemId || !itemType) return null;
  const data = event.payload.data && typeof event.payload.data === "object"
    ? event.payload.data as Payload
    : {};
  return { id: itemId, type: itemType, ...data };
}

function eventsToTurns(events: RunEvent[]) {
  const turns = new Map<string, { id: string; status: string; items: Payload[]; startedAt?: string }>();
  for (const event of [...events].sort((left, right) => left.sequence - right.sequence)) {
    const turnId = event.turn_id ?? "history";
    const turn = turns.get(turnId) ?? {
      id: turnId,
      status: "completed",
      items: [] as Payload[],
      startedAt: undefined as string | undefined,
    };
    if (event.event_type === "codex.turn.started") {
      turn.status = "inProgress";
      turn.startedAt = event.created_at;
    } else if (event.event_type === "codex.turn.completed") {
      turn.status = "completed";
    } else if (event.event_type === "codex.item.completed") {
      const item = rawItem(event);
      if (item) {
        const index = turn.items.findIndex((entry) => entry.id === item.id);
        if (index >= 0) turn.items[index] = item;
        else turn.items.push(item);
      }
    }
    turns.set(turnId, turn);
  }
  return [...turns.values()];
}

async function threadRecord(threadId: string) {
  const context = await findThreadContext(threadId);
  const [task, project, run, events] = await Promise.all([
    platformClient.getTask(context.taskId),
    platformClient.getProject(context.projectId),
    platformClient.getRun(context.runId),
    platformClient.listEvents(context.taskId),
  ]);
  return {
    id: threadId,
    name: task.title,
    preview: task.title,
    cwd: projectPath(project),
    createdAt: task.created_at,
    updatedAt: run.updated_at,
    activeTurnId: run.active_turn_id,
    status: run.active_turn_id ? "active" : run.status,
    turns: eventsToTurns(events.filter((event) => event.run_id === run.id)),
  };
}

function runtimeMessage(event: RunEvent): Payload | null {
  const data = event.payload.data && typeof event.payload.data === "object"
    ? event.payload.data as Payload
    : {};
  const base = {
    threadId: event.thread_id,
    ...(event.turn_id ? { turnId: event.turn_id } : {}),
  };
  if (event.event_type === "platform.approval.requested") {
    const requestMethod = typeof data.requestMethod === "string" ? data.requestMethod : null;
    const requestParams = data.requestParams && typeof data.requestParams === "object"
      ? data.requestParams as Payload
      : null;
    const approvalId = typeof data.approvalId === "string" ? data.approvalId : null;
    if (!requestMethod || !requestParams || !approvalId) return null;
    return {
      method: requestMethod,
      id: approvalId,
      params: { ...base, ...requestParams },
    };
  }
  if (event.event_type === "codex.item.started" || event.event_type === "codex.item.completed") {
    const item = rawItem(event);
    if (!item) return null;
    return {
      method: event.event_type === "codex.item.started" ? "item/started" : "item/completed",
      params: { ...base, item },
    };
  }
  if (event.event_type === "codex.item.delta") {
    const sourceType = data.sourceType;
    if (typeof sourceType !== "string") return null;
    return {
      method: sourceType,
      params: {
        ...base,
        itemId: event.item_id,
        delta: data.delta,
        summaryIndex: data.summaryIndex,
        contentIndex: data.contentIndex,
      },
    };
  }
  const method = {
    "codex.turn.started": "turn/started",
    "codex.turn.completed": "turn/completed",
    "codex.thread.started": "thread/started",
    "codex.thread.completed": "thread/completed",
    "codex.thread.failed": "thread/failed",
  }[event.event_type];
  return method ? { method, params: { ...base, ...data } } : null;
}

export function startPlatformEventBridge() {
  unsubscribeEvents?.();
  unsubscribeEvents = platformClient.subscribe(
    (event) => {
      if (event.event_type === "terminal.output") {
        const payload = event.payload as Payload;
        const workspaceId = payload.workspaceId;
        const terminalId = payload.terminalId;
        const data = payload.data;
        if (typeof workspaceId === "string" && typeof terminalId === "string" && typeof data === "string") {
          emitBrowserEvent("terminal-output", { workspaceId, terminalId, data });
        }
        return;
      }
      if (event.event_type === "terminal.exit") {
        const payload = event.payload as Payload;
        const workspaceId = payload.workspaceId;
        const terminalId = payload.terminalId;
        if (typeof workspaceId === "string" && typeof terminalId === "string") {
          emitBrowserEvent("terminal-exit", { workspaceId, terminalId });
        }
        return;
      }
      const message = runtimeMessage(event);
      if (!message || !event.thread_id) return;
      void findThreadContext(event.thread_id).then((context) => {
        const payload: AppServerEvent = { workspace_id: context.projectId, message };
        emitBrowserEvent("app-server-event", payload);
      }).catch(() => undefined);
    },
    () => undefined,
  );
  return () => {
    unsubscribeEvents?.();
    unsubscribeEvents = null;
  };
}

function register(name: string, handler: (payload: Payload) => Promise<unknown> | unknown) {
  registerBrowserCommand(name, async (payload) => await handler(payload));
}

function extractJsonObject(text: string): Record<string, unknown> | null {
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  try {
    const value = JSON.parse(text.slice(start, end + 1)) as unknown;
    return value && typeof value === "object" && !Array.isArray(value)
      ? value as Record<string, unknown>
      : null;
  } catch {
    return null;
  }
}

function normalizeWorktreeName(value: string) {
  const cleaned = value.trim().toLowerCase()
    .replace(/[^a-z0-9/_\s-]/g, "")
    .replace(/[\s_]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/[-/]+$/, "");
  const prefixes = ["feat/", "fix/", "chore/", "test/", "docs/", "refactor/", "perf/", "build/", "ci/", "style/"];
  const matched = prefixes.find((prefix) => cleaned.startsWith(prefix));
  if (matched) return cleaned;
  const dashed = prefixes.find((prefix) => cleaned.startsWith(prefix.replace("/", "-")));
  return dashed
    ? cleaned.replace(dashed.replace("/", "-"), dashed)
    : `feat/${cleaned.replace(/^\/+/, "")}`;
}

function parseRunMetadata(text: string) {
  const value = extractJsonObject(text);
  const title = typeof value?.title === "string" ? value.title.trim() : "";
  const rawName = typeof value?.worktreeName === "string"
    ? value.worktreeName
    : typeof value?.worktree_name === "string" ? value.worktree_name : "";
  if (!title || !rawName.trim()) throw new Error("Codex returned invalid run metadata");
  return { title, worktreeName: normalizeWorktreeName(rawName) };
}

function parseAgentDescription(text: string) {
  const value = extractJsonObject(text);
  const description = typeof value?.description === "string" ? value.description.trim() : "";
  const developerInstructions = typeof value?.developerInstructions === "string"
    ? value.developerInstructions.trim()
    : typeof value?.developer_instructions === "string"
      ? value.developer_instructions.trim()
      : "";
  if (!description && !developerInstructions) {
    throw new Error("Codex returned invalid Agent configuration");
  }
  return { description, developerInstructions };
}

async function dataUrlForBrowserResource(resource: string) {
  if (resource.startsWith("data:")) return resource;
  if (!/^(?:blob:|https?:)/i.test(resource)) {
    throw new Error("Browser image input is not an uploaded resource");
  }
  const response = await fetch(resource);
  if (!response.ok) throw new Error("Unable to read the selected image");
  const blob = await response.blob();
  return await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("Unable to encode the selected image"));
    reader.readAsDataURL(blob);
  });
}

export function registerPlatformBrowserCommands() {
  register("get_app_settings", () => loadAppSettings());
  register("update_app_settings", ({ settings }) => saveAppSettings(settings as AppSettings));
  register("is_mobile_runtime", () => false);
  register("app_build_type", () => import.meta.env.DEV ? "debug" : "release");
  register("is_macos_debug_build", () => false);
  register("menu_set_accelerators", () => undefined);
  register("set_tray_recent_threads", () => undefined);
  register("set_tray_session_usage", () => undefined);
  register("send_notification_fallback", () => undefined);
  register("read_image_as_data_url", ({ path }) => dataUrlForBrowserResource(requiredString({ path }, "path")));
  register("write_text_file", ({ path, content }) => {
    const name = requiredString({ path }, "path").split(/[\\/]/).pop() || "download.txt";
    const blob = new Blob([typeof content === "string" ? content : ""], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = name;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 0);
    return {};
  });

  register("list_workspaces", () => listBrowserWorkspaces());
  register("add_workspace", async ({ path }) => {
    const source = requiredString({ path }, "path");
    const name = source.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || "Workspace";
    return projectWorkspace(await platformClient.createProject(name, source, "main"));
  });
  register("add_workspace_from_git_url", async ({ url, targetFolderName }) => {
    const source = requiredString({ url }, "url");
    const fallback = source.replace(/\.git$/, "").split(/[/:]/).pop() || "Workspace";
    const name = typeof targetFolderName === "string" && targetFolderName.trim() ? targetFolderName.trim() : fallback;
    return projectWorkspace(await platformClient.createProject(name, source, "main"));
  });
  register("add_worktree", async ({ parentId, branch, name, copyAgentsMd }) => {
    return await createDerivedWorkspace(
      requiredString({ parentId }, "parentId"),
      requiredString({ branch }, "branch"),
      typeof name === "string" && name.trim() ? name.trim() : null,
      "worktree",
      copyAgentsMd !== false,
    );
  });
  register("add_clone", async ({ sourceWorkspaceId, copyName }) => {
    const sourceId = requiredString({ sourceWorkspaceId }, "sourceWorkspaceId");
    const { project, run } = await projectContextForWorkspace(sourceId);
    const sourceRun = run ? await platformClient.getRun(run.runId) : null;
    return await createDerivedWorkspace(
      sourceId,
      sourceRun?.source_ref ?? project.default_branch,
      requiredString({ copyName }, "copyName"),
      "clone",
    );
  });
  register("is_workspace_path_dir", ({ path }) => typeof path === "string" && path.trim().length > 0);
  register("connect_workspace", () => undefined);
  register("remove_workspace", async ({ id }) => { await platformClient.deleteProject(requiredString({ id }, "id")); });
  register("update_workspace_settings", async ({ id, settings }) => {
    const workspaceId = requiredString({ id }, "id");
    await platformClient.updateBrowserWorkspaceSettings(
      workspaceId,
      settings && typeof settings === "object" ? settings as Record<string, unknown> : {},
    );
    const workspace = (await listBrowserWorkspaces()).find((entry) => entry.id === workspaceId);
    if (!workspace) throw new Error("Workspace was not found after updating settings");
    return workspace;
  });
  register("set_workspace_runtime_codex_args", ({ workspaceId, codexArgs }) => platformClient.setWorkspaceRuntimeCodexArgs(
    requiredString({ workspaceId }, "workspaceId"),
    typeof codexArgs === "string" && codexArgs.trim() ? codexArgs.trim() : null,
  ));
  register("worktree_setup_status", ({ workspaceId }) => platformClient.worktreeSetupStatus(
    requiredString({ workspaceId }, "workspaceId"),
  ));
  register("worktree_setup_mark_ran", ({ workspaceId }) => platformClient.markWorktreeSetupRan(
    requiredString({ workspaceId }, "workspaceId"),
  ));
  register("remove_worktree", async ({ id }) => {
    const workspaceId = requiredString({ id }, "id");
    const context = await findWorkspaceRun(workspaceId);
    if (!context) throw new Error("Worktree workspace was not found");
    await platformClient.removeDerivedWorkspace(context.runId);
    workspaceRuns.delete(workspaceId);
    selectedWorkspaceRuns.delete(workspaceId);
    workspaceSettings.delete(workspaceId);
  });
  register("rename_worktree", async ({ id, branch }) => {
    const workspaceId = requiredString({ id }, "id");
    const context = await findWorkspaceRun(workspaceId);
    if (!context) throw new Error("Worktree workspace was not found");
    await platformClient.renameWorkspaceBranch(context.runId, requiredString({ branch }, "branch"));
    const workspaces = await listBrowserWorkspaces();
    const renamed = workspaces.find((workspace) => workspace.id === workspaceId);
    if (!renamed) throw new Error("Renamed worktree workspace was not found");
    return renamed;
  });
  register("rename_worktree_upstream", async ({ id, oldBranch, newBranch }) => {
    const workspaceId = requiredString({ id }, "id");
    const context = await findWorkspaceRun(workspaceId);
    if (!context) throw new Error("Worktree workspace was not found");
    return await platformClient.renameWorkspaceUpstream(
      context.runId,
      requiredString({ oldBranch }, "oldBranch"),
      requiredString({ newBranch }, "newBranch"),
    );
  });
  register("apply_worktree_changes", async ({ workspaceId }) => {
    const id = requiredString({ workspaceId }, "workspaceId");
    const context = await findWorkspaceRun(id);
    if (!context) throw new Error("Worktree workspace was not found");
    return await platformClient.applyDerivedWorkspace(context.runId);
  });

  register("start_thread", async ({ workspaceId }) => {
    const workspace = requiredString({ workspaceId }, "workspaceId");
    const { run: ready } = await createThreadRunInWorkspace(workspace);
    return { thread: await threadRecord(ready.codex_thread_id as string) };
  });
  register("list_threads", async ({ workspaceId }) => {
    const projectId = requiredString({ workspaceId }, "workspaceId");
    const existing = await findWorkspaceRun(projectId);
    if (existing) {
      const entries = (await indexAllProjectThreads(existing.projectId)).filter(({ run }) =>
        run.id === existing.runId || run.workspace_group_run_id === existing.runId)
        .filter(({ task }) => task.status !== "archived");
      return {
        data: await Promise.all(entries.map(({ threadId }) => threadRecord(threadId))),
        nextCursor: null,
      };
    }
    const data = (await indexProjectThreads(projectId))
      .filter(({ task }) => task.status !== "archived")
      .map(({ project, task, run, threadId }) => ({
        id: threadId,
        name: task.title,
        preview: task.title,
        cwd: projectPath(project),
        createdAt: task.created_at,
        updatedAt: run.updated_at,
        activeTurnId: run.active_turn_id,
        status: run.active_turn_id ? "active" : run.status,
      }));
    return { data, nextCursor: null };
  });
  for (const name of ["resume_thread", "read_thread"] as const) {
    register(name, async ({ workspaceId, threadId }) => {
      const id = requiredString({ threadId }, "threadId");
      const context = await findThreadContext(id);
      if (typeof workspaceId === "string" && workspaceId.trim()) {
        selectedWorkspaceRuns.set(workspaceId.trim(), context);
      }
      return { thread: await threadRecord(id) };
    });
  }
  register("fork_thread", async ({ workspaceId, threadId }) => {
    const workspace = requiredString({ workspaceId }, "workspaceId");
    const sourceThreadId = requiredString({ threadId }, "threadId");
    const { run } = await createThreadRunInWorkspace(workspace, sourceThreadId);
    const thread = await threadRecord(run.codex_thread_id as string);
    return { result: { thread } };
  });
  register("list_thread_turns", async ({ threadId }) => ({ data: (await threadRecord(requiredString({ threadId }, "threadId"))).turns, nextCursor: null }));
  register("send_user_message", async ({ threadId, text, model, effort, serviceTier, accessMode, images, collaborationMode }) => {
    const context = await findThreadContext(requiredString({ threadId }, "threadId"));
    return await platformClient.sendMessage(context.taskId, requiredString({ text }, "text"), {
      model: typeof model === "string" ? model : null,
      effort: typeof effort === "string" ? effort : null,
      serviceTier: typeof serviceTier === "string" ? serviceTier : null,
      accessMode: typeof accessMode === "string" ? accessMode : null,
      images: Array.isArray(images) ? images.filter((value): value is string => typeof value === "string") : [],
      collaborationMode: collaborationMode && typeof collaborationMode === "object"
        ? collaborationMode as Record<string, unknown>
        : null,
    });
  });
  register("turn_interrupt", async ({ threadId, turnId }) => {
    const context = await findThreadContext(requiredString({ threadId }, "threadId"));
    return await platformClient.interruptRun(context.runId, requiredString({ turnId }, "turnId"));
  });
  register("turn_steer", async ({ threadId, turnId, text, images }) => {
    const context = await findThreadContext(requiredString({ threadId }, "threadId"));
    return await platformClient.steerRun(
      context.runId,
      requiredString({ turnId }, "turnId"),
      requiredString({ text }, "text"),
      Array.isArray(images) ? images.filter((value): value is string => typeof value === "string") : [],
    );
  });
  register("compact_thread", async ({ threadId }) => {
    const context = await findThreadContext(requiredString({ threadId }, "threadId"));
    return await platformClient.compactRunThread(context.runId);
  });
  register("start_review", async ({ workspaceId, threadId, target, delivery }) => {
    const context = await findThreadContext(requiredString({ threadId }, "threadId"));
    if (!target || typeof target !== "object" || Array.isArray(target)) {
      throw new Error("review target is required");
    }
    if (delivery === "detached") {
      const workspace = requiredString({ workspaceId }, "workspaceId");
      const { run } = await createThreadRunInWorkspace(
        workspace,
        requiredString({ threadId }, "threadId"),
      );
      const result = await platformClient.startRunReview(
        run.id,
        target as Record<string, unknown>,
        "inline",
      );
      return {
        result: {
          ...result,
          reviewThreadId: run.codex_thread_id,
        },
      };
    }
    return await platformClient.startRunReview(context.runId, target as Record<string, unknown>, "inline");
  });
  register("archive_thread", async ({ threadId }) => {
    const context = await findThreadContext(requiredString({ threadId }, "threadId"));
    return await platformClient.archiveRunThread(context.runId);
  });
  register("set_thread_name", async ({ threadId, name }) => {
    const context = await findThreadContext(requiredString({ threadId }, "threadId"));
    return await platformClient.setRunThreadName(context.runId, requiredString({ name }, "name"));
  });
  register("thread_live_subscribe", () => ({}));
  register("thread_live_unsubscribe", () => ({}));

  register("model_provider_list", () => platformClient.listProviders());
  register("model_list", async () => {
    const catalog = await platformClient.listProviders();
    const provider = catalog.data.find((entry) => entry.id === catalog.currentProviderId);
    return { data: (provider?.models ?? []).map((model, index) => ({
      id: model.modelId,
      model: model.modelId,
      displayName: model.modelName ?? model.modelId,
      description: "",
      supportedReasoningEfforts: [],
      defaultReasoningEffort: null,
      isDefault: index === 0,
    })) };
  });
  register("model_provider_write", async ({ input }) => {
    const record = input as Payload;
    const id = requiredString(record, "id");
    const action = typeof record.action === "string" ? record.action : "upsert";
    if (action === "select") return await platformClient.selectProvider(id);
    if (action === "delete") return await platformClient.deleteProvider(id);
    if (action === "fetch") return await platformClient.refreshProviderModels(id);
    if (action === "context") {
      return await platformClient.updateProviderModel(
        id,
        requiredString(record, "modelId"),
        Number(record.contextWindow),
      );
    }
    const credentialMode = typeof record.credentialMode === "string" ? record.credentialMode : "preserve";
    const credentials = credentialMode === "environment"
      ? { mode: "environment", envKey: requiredString(record, "envKey") }
      : credentialMode === "direct"
        ? { mode: "direct", apiKey: requiredString(record, "apiKey") }
        : credentialMode === "none"
          ? { mode: "none" }
          : { mode: "preserve" };
    return await platformClient.upsertProvider(id, {
      name: requiredString(record, "name"),
      baseUrl: requiredString(record, "baseUrl"),
      wireApi: typeof record.wireApi === "string" ? record.wireApi : "responses",
      credentials,
      select: record.select === true,
    });
  });

  register("get_git_status", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    const status = await platformClient.workspaceStatus(run.id);
    const files = status.changes.map((change) => ({
      path: change.path,
      status: change.status,
      additions: change.additions ?? 0,
      deletions: change.deletions ?? 0,
    }));
    const stagedFiles = files.filter((file) => file.status !== "??" && file.status[0] !== " ");
    const unstagedFiles = files.filter((file) => file.status === "??" || file.status[1] !== " ");
    return {
      branchName: status.branch,
      files,
      stagedFiles,
      unstagedFiles,
      totalAdditions: files.reduce((sum, file) => sum + file.additions, 0),
      totalDeletions: files.reduce((sum, file) => sum + file.deletions, 0),
    };
  });
  register("list_workspace_files", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.listWorkspaceFiles(run.id);
  });
  register("read_workspace_file", async ({ workspaceId, path }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.readWorkspaceFile(run.id, requiredString({ path }, "path"));
  });
  register("get_git_diffs", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return (await platformClient.workspaceDiffs(run.id)).map((entry) => ({
      path: entry.path,
      diff: entry.diff,
      isBinary: entry.isBinary,
    }));
  });
  register("stage_git_file", async ({ workspaceId, path }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.stageWorkspacePaths(run.id, [requiredString({ path }, "path")]);
  });
  register("stage_git_all", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.stageAllWorkspacePaths(run.id);
  });
  register("unstage_git_file", async ({ workspaceId, path }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.unstageWorkspacePaths(run.id, [requiredString({ path }, "path")]);
  });
  register("revert_git_file", async ({ workspaceId, path }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.revertWorkspacePaths(run.id, [requiredString({ path }, "path")]);
  });
  register("revert_git_all", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.revertAllWorkspacePaths(run.id);
  });
  register("commit_git", async ({ workspaceId, message }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    const status = await platformClient.workspaceStatus(run.id);
    const staged = status.changes
      .filter((change) => change.status !== "??" && change.status[0] !== " ")
      .map((change) => change.path);
    const selected = staged.length > 0 ? staged : status.changes.map((change) => change.path);
    return await platformClient.commitWorkspace(run.id, selected, requiredString({ message }, "message"));
  });
  register("list_git_branches", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return { branches: await platformClient.listWorkspaceBranches(run.id) };
  });
  register("checkout_git_branch", async ({ workspaceId, name }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.checkoutWorkspaceBranch(run.id, requiredString({ name }, "name"));
  });
  register("create_git_branch", async ({ workspaceId, name }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.createWorkspaceBranch(run.id, requiredString({ name }, "name"));
  });
  register("get_git_log", async ({ workspaceId, limit }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.workspaceLog(
      run.id,
      typeof limit === "number" && Number.isFinite(limit) ? limit : 40,
    );
  });
  register("get_git_remote", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.workspaceRemote(run.id);
  });
  for (const operation of ["fetch", "pull", "push", "sync"] as const) {
    register(`${operation}_git`, async ({ workspaceId }) => {
      const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
      return await platformClient.workspaceRemoteOperation(run.id, operation);
    });
  }
  register("init_git_repo", () => ({ status: "already_initialized" }));
  register("create_github_repo", async ({ workspaceId, repo, visibility, branch }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    const selectedVisibility = requiredString({ visibility }, "visibility");
    if (selectedVisibility !== "private" && selectedVisibility !== "public") {
      throw new Error("visibility must be private or public");
    }
    return await platformClient.createGithubRepository(
      run.id,
      requiredString({ repo }, "repo"),
      selectedVisibility,
      typeof branch === "string" && branch.trim() ? branch.trim() : null,
    );
  });
  register("get_workspace_files", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.listWorkspaceFiles(run.id);
  });

  register("terminal_open", async ({ workspaceId, terminalId, cols, rows }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.openTerminal(
      run.id,
      requiredString({ terminalId }, "terminalId"),
      typeof cols === "number" ? cols : 80,
      typeof rows === "number" ? rows : 24,
    );
  });
  register("terminal_write", async ({ workspaceId, terminalId, data }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.writeTerminal(
      run.id,
      requiredString({ terminalId }, "terminalId"),
      requiredString({ data }, "data"),
    );
  });
  register("terminal_resize", async ({ workspaceId, terminalId, cols, rows }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.resizeTerminal(
      run.id,
      requiredString({ terminalId }, "terminalId"),
      typeof cols === "number" ? cols : 80,
      typeof rows === "number" ? rows : 24,
    );
  });
  register("terminal_close", async ({ workspaceId, terminalId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.closeTerminal(
      run.id,
      requiredString({ terminalId }, "terminalId"),
    );
  });

  register("respond_to_server_request", async ({ requestId, result }) => {
    const id = String(requestId ?? "");
    const approvals = await platformClient.listApprovals();
    const approval = approvals.find((entry) => entry.id === id);
    if (!approval) throw new Error("Approval is no longer pending");
    const response = result && typeof result === "object" ? result as Payload : {};
    if (response.answers && typeof response.answers === "object") {
      await platformClient.respondUserInput(
        id,
        response.answers as Record<string, { answers: string[] }>,
        approval.version,
      );
      return {};
    }
    const decision = response.decision === "accept" ? "accept" : "decline";
    await platformClient.decideApproval(id, decision, approval.version);
    return {};
  });
  register("remember_approval_rule", async ({ workspaceId, command }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.rememberApprovalRule(
      run.id,
      Array.isArray(command) ? command.filter((value): value is string => typeof value === "string") : [],
    );
  });

  register("account_rate_limits", () => platformClient.profileRateLimits());
  register("account_read", () => platformClient.profileAccount());
  register("list_mcp_server_status", async ({ workspaceId, cursor, limit }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.profileMcpServers(
      run.id,
      typeof cursor === "string" ? cursor : null,
      typeof limit === "number" ? limit : null,
    );
  });
  register("collaboration_mode_list", () => platformClient.profileCollaborationModes());
  register("skills_list", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.profileSkills(run.id);
  });
  register("apps_list", async ({ workspaceId, cursor, limit }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.profileApps(
      run.id,
      typeof cursor === "string" ? cursor : null,
      typeof limit === "number" ? limit : null,
    );
  });
  register("prompts_list", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.listPrompts(run.id);
  });
  register("prompts_workspace_dir", () => "profile://prompts/workspace");
  register("prompts_global_dir", () => "profile://prompts/global");
  register("prompts_create", async (payload) => {
    const run = await runForWorkspace(requiredString(payload, "workspaceId"));
    return await platformClient.createPrompt({
      runId: run.id,
      scope: requiredString(payload, "scope"),
      name: requiredString(payload, "name"),
      description: typeof payload.description === "string" ? payload.description : null,
      argumentHint: typeof payload.argumentHint === "string" ? payload.argumentHint : null,
      content: typeof payload.content === "string" ? payload.content : "",
    });
  });
  register("prompts_update", async (payload) => {
    const run = await runForWorkspace(requiredString(payload, "workspaceId"));
    return await platformClient.updatePrompt({
      runId: run.id,
      path: requiredString(payload, "path"),
      name: requiredString(payload, "name"),
      description: typeof payload.description === "string" ? payload.description : null,
      argumentHint: typeof payload.argumentHint === "string" ? payload.argumentHint : null,
      content: typeof payload.content === "string" ? payload.content : "",
    });
  });
  register("prompts_delete", async ({ workspaceId, path }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.deletePrompt({ runId: run.id, path: requiredString({ path }, "path") });
  });
  register("prompts_move", async ({ workspaceId, path, scope }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.movePrompt({
      runId: run.id,
      path: requiredString({ path }, "path"),
      scope: requiredString({ scope }, "scope"),
    });
  });
  register("experimental_feature_list", async ({ workspaceId, cursor, limit }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.profileExperimentalFeatures(
      run.id,
      typeof cursor === "string" ? cursor : null,
      typeof limit === "number" ? limit : null,
    );
  });
  register("set_codex_feature_flag", ({ featureKey, enabled }) => platformClient.setExperimentalFeature(
    requiredString({ featureKey }, "featureKey"),
    enabled === true,
  ));
  register("file_read", async ({ scope, kind, workspaceId }) => {
    if (scope === "global" && (kind === "agents" || kind === "config")) {
      return await platformClient.readProfileFile(kind);
    }
    if (scope === "workspace" && kind === "agents") {
      const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
      try {
        const file = await platformClient.readWorkspaceFile(run.id, "AGENTS.md");
        return { exists: true, content: file.content, truncated: file.truncated };
      } catch {
        return { exists: false, content: "", truncated: false };
      }
    }
    throw new Error("Unsupported Profile file scope");
  });
  register("file_write", async ({ scope, kind, content, workspaceId }) => {
    if (scope === "global" && (kind === "agents" || kind === "config")) {
      return await platformClient.writeProfileFile(kind, typeof content === "string" ? content : "");
    }
    if (scope === "workspace" && kind === "agents") {
      const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
      return await platformClient.writeWorkspaceAgents(run.id, typeof content === "string" ? content : "");
    }
    throw new Error("Unsupported Profile file scope");
  });
  register("get_agents_settings", () => platformClient.getAgents());
  register("set_agents_core_settings", ({ input }) => {
    const record = input && typeof input === "object" ? input as Payload : {};
    return platformClient.setAgentsCore({
      multiAgentEnabled: record.multiAgentEnabled === true,
      maxThreads: Number(record.maxThreads),
      maxDepth: Number(record.maxDepth),
    });
  });
  register("create_agent", ({ input }) => platformClient.createAgent(
    input && typeof input === "object" ? input as Payload : {},
  ));
  register("update_agent", ({ input }) => {
    const record = input && typeof input === "object" ? input as Payload : {};
    const originalName = requiredString(record, "originalName");
    return platformClient.updateAgent(originalName, {
      name: requiredString(record, "name"),
      description: typeof record.description === "string" ? record.description : null,
      developerInstructions: typeof record.developerInstructions === "string" ? record.developerInstructions : null,
      renameManagedFile: record.renameManagedFile !== false,
    });
  });
  register("delete_agent", ({ input }) => {
    const record = input && typeof input === "object" ? input as Payload : {};
    return platformClient.deleteAgent(
      requiredString(record, "name"),
      record.deleteManagedFile === true,
    );
  });
  register("read_agent_config_toml", ({ agentName }) => platformClient.readAgentConfig(
    requiredString({ agentName }, "agentName"),
  ));
  register("write_agent_config_toml", ({ agentName, content }) => platformClient.writeAgentConfig(
    requiredString({ agentName }, "agentName"),
    typeof content === "string" ? content : "",
  ));
  register("generate_run_metadata", async ({ workspaceId, prompt }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    const response = await platformClient.generateText(
      run.id,
      "runMetadata",
      requiredString({ prompt }, "prompt"),
    );
    return parseRunMetadata(response.text);
  });
  register("generate_agent_description", async ({ workspaceId, description }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    const response = await platformClient.generateText(
      run.id,
      "agentDescription",
      requiredString({ description }, "description"),
    );
    return parseAgentDescription(response.text);
  });
  register("generate_commit_message", async ({ workspaceId, commitMessageModelId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    const response = await platformClient.generateText(
      run.id,
      "commitMessage",
      "",
      typeof commitMessageModelId === "string" ? commitMessageModelId : null,
    );
    return response.text.trim();
  });
  register("local_usage_snapshot", async ({ days, workspacePath }) => {
    let workspaceId: string | null = null;
    if (typeof workspacePath === "string" && workspacePath.trim()) {
      const workspace = (await listBrowserWorkspaces()).find(
        (entry) => entry.path === workspacePath.trim(),
      );
      workspaceId = workspace?.id ?? null;
    }
    return await platformClient.profileUsage(
      Math.min(90, Math.max(1, Number(days) || 30)),
      workspaceId,
    );
  });
  register("get_config_model", () => platformClient.getConfigModel());
  register("get_codex_config_path", () => "Profile-managed Codex configuration");
  register("get_open_app_icon", () => null);
  register("is_workspace_path_dir", ({ path }) => typeof path === "string" && path.length > 0);
  register("dictation_model_status", ({ modelId }) => browserDictationModelStatus(modelId));
  register("dictation_download_model", ({ modelId }) => browserDictationModelStatus(modelId));
  register("dictation_cancel_download", ({ modelId }) => browserDictationModelStatus(modelId));
  register("dictation_remove_model", ({ modelId }) => browserDictationModelStatus(modelId));
  register("dictation_request_permission", () => requestBrowserDictationPermission());
  register("dictation_start", ({ preferredLanguage }) => startBrowserDictation(preferredLanguage));
  register("dictation_stop", () => stopBrowserDictation());
  register("dictation_cancel", () => cancelBrowserDictation());
  register("app_build_type", () => import.meta.env.DEV ? "debug" : "release");
  register("codex_doctor", async () => {
    const health = await platformClient.health();
    return { ok: health.ok, codexBin: null, version: health.version, appServerOk: health.ok, details: null, path: null, nodeOk: true, nodeVersion: null, nodeDetails: null };
  });
  register("codex_login", async ({ workspaceId }) => {
    const browserWorkspaceId = requiredString({ workspaceId }, "workspaceId");
    const login = await platformClient.startProfileLogin();
    void watchProfileLogin(browserWorkspaceId, login.loginId);
    return login;
  });
  register("codex_login_cancel", async ({ workspaceId }) => {
    const browserWorkspaceId = requiredString({ workspaceId }, "workspaceId");
    const canceled = await platformClient.cancelProfileLogin();
    stopProfileLoginWatch(browserWorkspaceId);
    return canceled;
  });
  register("open_workspace_in", async ({ path }) => {
    const target = requiredString({ path }, "path");
    if (target.startsWith("https://") || target.startsWith("http://")) {
      await openUrl(target);
      return {};
    }
    const githubSsh = target.match(/^git@github\.com:([^/]+)\/(.+?)(?:\.git)?$/);
    if (githubSsh) {
      await openUrl(`https://github.com/${githubSsh[1]}/${githubSsh[2]}`);
      return {};
    }
    throw new Error(
      "Server workspace files cannot be opened in a desktop application from the browser.",
    );
  });
  register("codex_update", async () => {
    const health = await platformClient.health();
    return {
      ok: false,
      method: "unknown",
      package: null,
      beforeVersion: health.version,
      afterVersion: health.version,
      upgraded: false,
      output: null,
      details: "Codex runtime updates are managed by the Web deployment.",
    };
  });
  const managedDaemonStatus = () => ({
    state: "stopped",
    pid: null,
    startedAtMs: null,
    lastError: "Tailscale daemon lifecycle is managed outside the Web application.",
    listenAddr: null,
  });
  register("tailscale_daemon_start", managedDaemonStatus);
  register("tailscale_daemon_stop", managedDaemonStatus);

  register("get_git_commit_diff", async ({ workspaceId, sha }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return (await platformClient.workspaceCommitDiffs(
      run.id,
      requiredString({ sha }, "sha"),
    )).map((diff) => ({
      path: diff.path,
      status: diff.status,
      diff: diff.diff,
      isBinary: diff.isBinary,
      isImage: false,
    }));
  });
  register("list_git_roots", async ({ workspaceId, depth }) => {
    const browserWorkspaceId = requiredString({ workspaceId }, "workspaceId");
    const run = await rawRunForWorkspace(browserWorkspaceId);
    await platformClient.setWorkspaceGitRoot(run.id, null);
    return await platformClient.listWorkspaceGitRoots(
      run.id,
      Math.min(6, Math.max(1, Number(depth) || 2)),
    );
  });
  register("get_github_issues", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.githubIssues(run.id);
  });
  register("get_github_pull_requests", async ({ workspaceId }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.githubPullRequests(run.id);
  });
  register("get_github_pull_request_diff", async ({ workspaceId, prNumber }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.githubPullRequestDiff(run.id, Number(prNumber));
  });
  register("get_github_pull_request_comments", async ({ workspaceId, prNumber }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.githubPullRequestComments(run.id, Number(prNumber));
  });
  register("checkout_github_pull_request", async ({ workspaceId, prNumber }) => {
    const run = await runForWorkspace(requiredString({ workspaceId }, "workspaceId"));
    return await platformClient.checkoutGithubPullRequest(run.id, Number(prNumber));
  });
  register("tailscale_status", () => ({ installed: false, running: false, version: null, dnsName: null, hostName: null, tailnetName: null, ipv4: [], ipv6: [], suggestedRemoteHost: null, message: "Managed by the Web platform" }));
  register("tailscale_daemon_status", () => ({ state: "stopped", pid: null, startedAtMs: null, lastError: null, listenAddr: null }));
  register("tailscale_daemon_command_preview", () => ({ command: "", daemonPath: "", args: [], tokenConfigured: false }));
}

registerPlatformBrowserCommands();
