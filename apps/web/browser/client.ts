import type {
  Approval,
  AgentDefinitionSummary,
  AgentDefinitionDetail,
  AgentDefinitionDraftRequest,
  AgentDefinitionReleaseSummary,
  AgentDefinitionResourceSummary,
  AgentDefinitionValidationResult,
  AgentRunSelection,
  CapabilityPackageSummary,
  CapabilityDraftDetail,
  CapabilityDraftSummary,
  CapabilityInstallationSummary,
  CapabilityReleaseSummary,
  CapabilityReadinessSummary,
  CapabilityValidationResult,
  CatalogDraftContent,
  CatalogResourceKind,
  CollaborationStatusSummary,
  ProviderCallMetric,
  PythonCapabilityPublishRequest,
  PythonCapabilityPublishResponse,
  PythonCapabilityToolTestRequest,
  PythonCapabilityToolTestResponse,
  PythonCapabilityValidationResult,
  Me,
  Project,
  ProviderCatalog,
  Run,
  RunReadiness,
  RunReadinessRequest,
  TaskAnalysisReadinessRequest,
  DataIntakeSessionSummary,
  DataIntakeResponseRequest,
  AnalysisStartRequest,
  AnalysisStartResponse,
  WorkspaceDataDraftSummary,
  SourceAssetSummary,
  RunEvent,
  RuntimeAgentActivity,
  RuntimeAgentExecution,
  PendingUserInputSummary,
  Session,
  Task,
  Workspace,
  WorkspaceStatus,
  WorkspaceFileContent,
  WorkspaceFileUploadResponse,
  WorkspaceFileDiff,
  PublishWorkspaceDatasetRequest,
  WorkspaceDatasetReleaseSummary,
  TutorialBlueprint,
  TutorialBlueprintReconcileResponse,
  TutorialBlueprintSummary,
  WorkspaceBranch,
  WorkspaceLog,
  WorkspaceCommitDiff,
  ProfileTextFile,
  AgentsSettings,
  PromptEntry,
  GitHubIssues,
  GitHubPullRequests,
  GitHubPullRequestDiff,
  GitHubPullRequestComment,
  ProfileLoginStart,
  ProfileLoginCancel,
  ProfileLoginStatus,
  ProjectThreadContext,
  ThreadHistoryResponse,
  ThreadHistoryTurn,
  BrowserWorkspacePreference,
  CreateGitHubRepositoryResponse,
  MapsConfiguration,
  MapsProvider,
  SupervisorInstructionPolicyDetail,
  SupervisorInstructionPolicyPublishRequest,
  SupervisorInstructionPolicySummary,
  SupervisorPolicyBinding,
  SupervisorPolicyDetail,
  SupervisorPolicySelection,
  SupervisorPolicySummary,
  SupervisorDefinitionSummary,
  SupervisorDraftRequest,
  SupervisorReleaseSummary,
  SupervisorValidationResult,
  RuntimeAgentProjection,
  ArtifactSummary,
} from "./types";

type ClientOptions = {
  baseUrl?: string;
  token?: string;
};

type LiveEnvelope =
  | { type: "ready"; version: number }
  | { type: "run.event"; version: number; event: RunEvent }
  | { type: "resyncRequired"; version: number }
  | { type: "error"; version: number; code: string };

function defaultBaseUrl() {
  return (import.meta.env.VITE_PLATFORM_API_URL ?? "").replace(/\/$/, "");
}

const createIdempotencyKey = () => {
  const randomUUID = globalThis.crypto?.randomUUID;
  if (typeof randomUUID === "function") {
    return randomUUID.call(globalThis.crypto);
  }
  return `idempotency-${Date.now()}-${Math.random().toString(16).slice(2)}`;
};

export class PlatformClient {
  private baseUrl: string;
  private token: string;

  constructor(options: ClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? defaultBaseUrl()).replace(/\/$/, "");
    this.token = options.token?.trim() ?? "";
  }

  setToken(token: string) {
    this.token = token.trim();
  }

  setBaseUrl(baseUrl: string) {
    this.baseUrl = baseUrl.trim().replace(/\/$/, "");
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const isFormData =
      typeof FormData !== "undefined" && init?.body instanceof FormData;
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      cache: "no-store",
      headers: {
        ...(init?.body && !isFormData ? { "content-type": "application/json" } : {}),
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
        ...init?.headers,
      },
    });
    const text = await response.text();
    let payload: unknown = null;
    if (text) {
      try {
        payload = JSON.parse(text) as unknown;
      } catch {
        throw new Error(
          `Server returned a non-JSON response for ${path} (HTTP ${response.status}, ${response.url}).`,
        );
      }
    }
    if (!response.ok) {
      const record = payload && typeof payload === "object"
        ? payload as Record<string, unknown>
        : null;
      const message = typeof record?.message === "string"
        ? record.message
        : `Request failed (HTTP ${response.status}).`;
      const error = new Error(message) as Error & {
        code?: string;
        status?: number;
        kind?: string;
      };
      error.name = "PlatformRequestError";
      error.code = message;
      error.status = response.status;
      error.kind = typeof record?.kind === "string" ? record.kind : undefined;
      throw error;
    }
    return payload as T;
  }

  health() {
    return this.request<{ ok: boolean; schemaStatus: "current" | "not_current"; version: string }>("/api/health");
  }

  bootstrap(name: string, username: string, email: string, password: string) {
    return this.request<Session>("/api/bootstrap", {
      method: "POST",
      body: JSON.stringify({ name, username, email, password }),
    });
  }

  login(username: string, password: string) {
    return this.request<Session>("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ username, password }),
    });
  }

  createLocalSession() {
    return this.request<Session>("/api/sessions/local", {
      method: "POST",
    });
  }

  me() {
    return this.request<Me>("/api/me");
  }

  listBrowserWorkspacePreferences() {
    return this.request<BrowserWorkspacePreference[]>("/api/browser-workspace-preferences");
  }

  updateBrowserWorkspaceSettings(workspaceId: string, settings: Record<string, unknown>) {
    return this.request<BrowserWorkspacePreference>(
      `/api/browser-workspace-preferences/${encodeURIComponent(workspaceId)}`,
      { method: "PUT", body: JSON.stringify({ settings }) },
    );
  }

  setWorkspaceRuntimeCodexArgs(workspaceId: string, codexArgs: string | null) {
    return this.request<{ appliedCodexArgs: string | null; respawned: boolean }>(
      `/api/browser-workspace-preferences/${encodeURIComponent(workspaceId)}/runtime-codex-args`,
      { method: "PUT", body: JSON.stringify({ codexArgs }) },
    );
  }

  worktreeSetupStatus(workspaceId: string) {
    return this.request<{ shouldRun: boolean; script: string | null }>(
      `/api/browser-workspace-preferences/${encodeURIComponent(workspaceId)}/worktree-setup`,
    );
  }

  markWorktreeSetupRan(workspaceId: string) {
    return this.request<{ status: string }>(
      `/api/browser-workspace-preferences/${encodeURIComponent(workspaceId)}/worktree-setup`,
      { method: "POST" },
    );
  }

  getMapsConfiguration() {
    return this.request<MapsConfiguration>("/api/configuration/maps");
  }

  updateMapsConfiguration(
    provider: MapsProvider,
    apiKey: string,
    elicitationUrl?: string,
  ) {
    return this.request<MapsConfiguration>("/api/configuration/maps", {
      method: "PUT",
      body: JSON.stringify({ provider, apiKey, elicitationUrl }),
    });
  }

  useMapsConfiguration(elicitationUrl: string) {
    return this.request<MapsConfiguration>("/api/configuration/maps/use", {
      method: "POST",
      body: JSON.stringify({ elicitationUrl }),
    });
  }

  listProjects() {
    return this.request<Project[]>("/api/projects");
  }

  getProject(projectId: string) {
    return this.request<Project>(`/api/projects/${encodeURIComponent(projectId)}`);
  }

  listProjectThreadContexts(projectId: string) {
    return this.request<ProjectThreadContext[]>(
      `/api/projects/${encodeURIComponent(projectId)}/thread-contexts`,
    );
  }

  deleteProject(projectId: string) {
    return this.request<{ deleted: boolean; id: string }>(
      `/api/projects/${encodeURIComponent(projectId)}`,
      { method: "DELETE" },
    );
  }

  createProject(name: string, gitUrl: string, defaultBranch: string) {
    return this.request<Project>("/api/projects", {
      method: "POST",
      body: JSON.stringify({ name, git_url: gitUrl, default_branch: defaultBranch }),
    });
  }

  createManagedProject(name: string) {
    return this.request<Project>("/api/projects/managed", {
      method: "POST",
      body: JSON.stringify({ name }),
    });
  }

  listWorkspaces() {
    return this.request<Workspace[]>("/api/workspaces");
  }

  getWorkspace(workspaceId: string) {
    return this.request<Workspace>(`/api/workspaces/${encodeURIComponent(workspaceId)}`);
  }

  createWorkspace(input: {
    projectId: string;
    kind: Workspace["kind"];
    name?: string | null;
    sourceRef?: string | null;
    parentWorkspaceId?: string | null;
    copyAgentsMd?: boolean;
  }) {
    return this.request<Workspace>("/api/workspaces", {
      method: "POST",
      body: JSON.stringify({
        project_id: input.projectId,
        idempotency_key: createIdempotencyKey(),
        kind: input.kind,
        name: input.name?.trim() || null,
        source_ref: input.sourceRef?.trim() || null,
        parent_workspace_id: input.parentWorkspaceId ?? null,
        copy_agents_md: input.copyAgentsMd ?? false,
      }),
    });
  }

  removeWorkspace(workspaceId: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}`,
      { method: "DELETE" },
    );
  }

  listTasks(projectId: string) {
    return this.request<Task[]>(`/api/tasks?project_id=${encodeURIComponent(projectId)}`);
  }

  createTask(
    projectId: string,
    title: string,
    selection?: { providerId: string; modelId: string } | null,
  ) {
    return this.request<Task>("/api/tasks", {
      method: "POST",
      body: JSON.stringify({
        project_id: projectId,
        title,
        model_provider: selection?.providerId ?? null,
        model: selection?.modelId ?? null,
      }),
    });
  }

  updateTaskModelSelection(taskId: string, providerId: string, modelId: string) {
    return this.request<{ providerId: string; modelId: string }>(
      `/api/tasks/${encodeURIComponent(taskId)}/model-selection`,
      {
        method: "PUT",
        body: JSON.stringify({ providerId, modelId }),
      },
    );
  }

  getTask(taskId: string) {
    return this.request<Task>(`/api/tasks/${encodeURIComponent(taskId)}`);
  }

  listRuns(taskId: string) {
    return this.request<Run[]>(`/api/runs?task_id=${encodeURIComponent(taskId)}`);
  }

  getRun(runId: string) {
    return this.request<Run>(`/api/runs/${encodeURIComponent(runId)}`);
  }

  readRunThread(runId: string) {
    return this.request<ThreadHistoryResponse>(`/api/runs/${encodeURIComponent(runId)}/thread`);
  }

  listRunThreadTurns(runId: string) {
    return this.request<ThreadHistoryTurn[]>(
      `/api/runs/${encodeURIComponent(runId)}/thread/turns`,
    );
  }

  listRunAgentThreadTurns(runId: string, threadId: string) {
    return this.request<ThreadHistoryTurn[]>(
      `/api/runs/${encodeURIComponent(runId)}/agents/${encodeURIComponent(threadId)}/turns`,
    );
  }

  readReplyArtifact(path: string) {
    if (!/^\/api\/artifacts\/[0-9a-f-]{36}\/content$/i.test(path)) {
      return Promise.reject(new Error("Reply Artifact path is invalid."));
    }
    return this.request<Record<string, unknown>>(path);
  }

  readArtifactContent(artifactId: string) {
    return this.readReplyArtifact(
      `/api/artifacts/${encodeURIComponent(artifactId)}/content`,
    );
  }

  archiveRunThread(runId: string) {
    return this.request<{ status: string }>(
      `/api/runs/${encodeURIComponent(runId)}/thread/archive`,
      { method: "POST" },
    );
  }

  setRunThreadName(runId: string, name: string) {
    return this.request<{ status: string; name: string }>(
      `/api/runs/${encodeURIComponent(runId)}/thread/name`,
      { method: "PUT", body: JSON.stringify({ name }) },
    );
  }

  startRun(
    taskId: string,
    workspaceId: string,
    options: {
      readinessFingerprint: string;
      idempotencyKey?: string;
      forkThreadId?: string | null;
      forkSourceRunId?: string | null;
      supervisorPolicy?: SupervisorPolicySelection | null;
      supervisorDraftId?: string | null;
      agent?: AgentRunSelection | null;
      purpose?: "conversation" | "analysis";
    },
  ) {
    return this.request<{ run: Run }>(`/api/tasks/${encodeURIComponent(taskId)}/runs`, {
      method: "POST",
      body: JSON.stringify({
        idempotency_key: options.idempotencyKey ?? createIdempotencyKey(),
        readiness_fingerprint: options.readinessFingerprint,
        workspace_id: workspaceId,
        fork_thread_id: options.forkThreadId ?? null,
        fork_source_run_id: options.forkSourceRunId ?? null,
        supervisor_policy: options.supervisorPolicy ?? null,
        supervisor_draft_id: options.supervisorDraftId ?? null,
        agent: options.agent ?? null,
        ...(options.purpose ? { purpose: options.purpose } : {}),
      }),
    });
  }

  evaluateRunReadiness(workspaceId: string, request: RunReadinessRequest) {
    return this.request<RunReadiness>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/run-readiness`,
      { method: "POST", body: JSON.stringify(request) },
    );
  }

  evaluateAnalysisReadiness(
    taskId: string,
    workspaceId: string,
    request: Omit<RunReadinessRequest, "purpose" | "task_id">,
  ) {
    const body: TaskAnalysisReadinessRequest = {
      workspace_id: workspaceId,
      ...request,
      purpose: "analysis",
      task_id: taskId,
    };
    return this.request<RunReadiness>(
      `/api/tasks/${encodeURIComponent(taskId)}/analysis-readiness`,
      { method: "POST", body: JSON.stringify(body) },
    );
  }

  createDataDraft(
    workspaceId: string,
    files: File[],
    idempotencyKey = createIdempotencyKey(),
  ) {
    const body = new FormData();
    for (const file of files) body.append("files", file, file.name);
    return this.request<WorkspaceDataDraftSummary>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/data-drafts`,
      {
        method: "POST",
        headers: {
          "idempotency-key": idempotencyKey,
        },
        body,
      },
    );
  }

  /** Upload a Workspace data draft while exposing the browser's real byte
   * progress.  The ordinary fetch-based method remains the small reusable
   * API used by non-composer callers; the Composer uses this method so its
   * progress indicator cannot pretend that an upload has finished early. */
  uploadDataDraft(
    workspaceId: string,
    files: File[],
    onProgress?: (percent: number) => void,
    idempotencyKey = createIdempotencyKey(),
  ): Promise<WorkspaceDataDraftSummary> {
    if (typeof XMLHttpRequest === "undefined") {
      return this.createDataDraft(workspaceId, files, idempotencyKey);
    }
    const body = new FormData();
    for (const file of files) body.append("files", file, file.name);
    return new Promise((resolve, reject) => {
      const request = new XMLHttpRequest();
      request.open(
        "POST",
        `${this.baseUrl}/api/workspaces/${encodeURIComponent(workspaceId)}/data-drafts`,
      );
      request.responseType = "text";
      if (this.token) request.setRequestHeader("authorization", `Bearer ${this.token}`);
      request.setRequestHeader("idempotency-key", idempotencyKey);
      request.upload.onprogress = (event) => {
        if (!event.lengthComputable) return;
        onProgress?.(Math.max(0, Math.min(100, Math.round((event.loaded / event.total) * 100))));
      };
      request.onerror = () => reject(new Error("The data upload failed."));
      request.onabort = () => reject(new Error("The data upload was cancelled."));
      request.onload = () => {
        let payload: unknown = null;
        if (request.responseText) {
          try {
            payload = JSON.parse(request.responseText) as unknown;
          } catch {
            reject(new Error(`Server returned a non-JSON response for data upload (HTTP ${request.status}).`));
            return;
          }
        }
        if (request.status < 200 || request.status >= 300) {
          const record = payload && typeof payload === "object"
            ? payload as Record<string, unknown>
            : null;
          const error = new Error(
            typeof record?.message === "string"
              ? record.message
              : `Data upload failed (HTTP ${request.status}).`,
          ) as Error & { code?: string; status?: number };
          error.name = "PlatformRequestError";
          error.code = typeof record?.code === "string" ? record.code : error.message;
          error.status = request.status;
          reject(error);
          return;
        }
        onProgress?.(100);
        resolve(payload as WorkspaceDataDraftSummary);
      };
      request.send(body);
    });
  }

  getDataIntake(taskId: string) {
    return this.request<DataIntakeSessionSummary>(
      `/api/tasks/${encodeURIComponent(taskId)}/data-intake`,
    );
  }

  listTutorialBlueprints() {
    return this.request<TutorialBlueprintSummary[]>("/api/tutorial-blueprints");
  }

  getTutorialBlueprint(blueprintId: string, revision: string) {
    return this.request<TutorialBlueprint>(
      `/api/tutorial-blueprints/${encodeURIComponent(blueprintId)}/${encodeURIComponent(revision)}`,
    );
  }

  reconcileTutorialBlueprint(
    workspaceId: string,
    blueprintId: string,
    revision: string,
    idempotencyKey: string,
  ) {
    return this.request<TutorialBlueprintReconcileResponse>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/tutorial-blueprints/${encodeURIComponent(blueprintId)}/${encodeURIComponent(revision)}/reconcile`,
      {
        method: "POST",
        body: JSON.stringify({ idempotency_key: idempotencyKey }),
      },
    );
  }

  listSupervisorPolicies() {
    return this.request<SupervisorPolicySummary[]>("/api/supervisor-policies");
  }

  getSupervisorPolicy(policyId: string, version: string) {
    return this.request<SupervisorPolicyDetail>(
      `/api/supervisor-policies/${encodeURIComponent(policyId)}/${encodeURIComponent(version)}`,
    );
  }

  listSupervisorInstructionPolicies() {
    return this.request<SupervisorInstructionPolicySummary[]>(
      "/api/supervisor-instruction-policies",
    );
  }

  getSupervisorInstructionPolicy(policyId: string, version: string) {
    return this.request<SupervisorInstructionPolicyDetail>(
      `/api/supervisor-instruction-policies/${encodeURIComponent(policyId)}/${encodeURIComponent(version)}`,
    );
  }

  publishSupervisorInstructionPolicy(
    request: SupervisorInstructionPolicyPublishRequest,
  ) {
    return this.request<SupervisorInstructionPolicyDetail>(
      "/api/supervisor-instruction-policies",
      { method: "POST", body: JSON.stringify(request) },
    );
  }

  listAgentDefinitions() {
    return this.request<AgentDefinitionSummary[]>("/api/agent-definitions");
  }

  listCapabilityPackages() {
    return this.request<CapabilityPackageSummary[]>("/api/capability-packages");
  }

  listCapabilityDrafts() {
    return this.request<CapabilityDraftSummary[]>("/api/catalog/drafts");
  }

  getCapabilityDraft(id: string) {
    return this.request<CapabilityDraftDetail>(`/api/catalog/drafts/${encodeURIComponent(id)}`);
  }

  createCapabilityDraft(input: {
    kind: CatalogResourceKind;
    resourceId: string;
    displayName: string;
    description: string;
    content: CatalogDraftContent;
  }) {
    return this.request<CapabilityDraftDetail>("/api/catalog/drafts", {
      method: "POST",
      body: JSON.stringify({
        kind: input.kind,
        resourceId: input.resourceId,
        displayName: input.displayName,
        description: input.description,
        content: input.content,
      }),
    });
  }

  saveCapabilityDraft(id: string, input: {
    expectedRevision: number;
    displayName: string;
    description: string;
    content: CatalogDraftContent;
  }) {
    return this.request<CapabilityDraftDetail>(
      `/api/catalog/drafts/${encodeURIComponent(id)}/save`,
      {
        method: "PUT",
        body: JSON.stringify({
          expectedRevision: input.expectedRevision,
          displayName: input.displayName,
          description: input.description,
          content: input.content,
        }),
      },
    );
  }

  validateCapabilityDraft(id: string) {
    return this.request<CapabilityValidationResult>(
      `/api/catalog/drafts/${encodeURIComponent(id)}/validate`,
      { method: "POST" },
    );
  }

  publishCapabilityDraft(id: string, expectedRevision: number) {
    return this.request<CapabilityReleaseSummary>(
      `/api/catalog/drafts/${encodeURIComponent(id)}/publish`,
      { method: "POST", body: JSON.stringify({ expectedRevision }) },
    );
  }

  listCapabilityReleases() {
    return this.request<CapabilityReleaseSummary[]>("/api/catalog/releases");
  }

  installCapabilityRelease(releaseId: string, workspaceId: string) {
    return this.request<CapabilityInstallationSummary>(
      `/api/catalog/releases/${encodeURIComponent(releaseId)}/install`,
      { method: "POST", body: JSON.stringify({ workspaceId }) },
    );
  }

  getCapabilityReadiness(releaseId: string, workspaceId: string) {
    return this.request<CapabilityReadinessSummary>(
      `/api/catalog/releases/${encodeURIComponent(releaseId)}/workspaces/${encodeURIComponent(workspaceId)}/readiness`,
    );
  }

  validatePythonCapability(
    workspaceId: string,
    request: PythonCapabilityPublishRequest,
  ) {
    return this.request<PythonCapabilityValidationResult>(
      `/api/workspaces/${workspaceId}/python-capabilities/validate`,
      { method: "POST", body: JSON.stringify(request) },
    );
  }

  testPythonCapability(
    workspaceId: string,
    request: PythonCapabilityToolTestRequest,
  ) {
    return this.request<PythonCapabilityToolTestResponse>(
      `/api/workspaces/${workspaceId}/python-capabilities/test`,
      { method: "POST", body: JSON.stringify(request) },
    );
  }

  publishPythonCapability(
    workspaceId: string,
    request: PythonCapabilityPublishRequest,
  ) {
    return this.request<PythonCapabilityPublishResponse>(
      `/api/workspaces/${workspaceId}/python-capabilities/publish`,
      { method: "POST", body: JSON.stringify(request) },
    );
  }

  getAgentDefinition(definitionId: string, version: string) {
    return this.request<AgentDefinitionDetail>(
      `/api/agent-definitions/${encodeURIComponent(definitionId)}/${encodeURIComponent(version)}`,
    );
  }

  listAgentDefinitionResources() {
    return this.request<AgentDefinitionResourceSummary[]>("/api/agent-definition-resources");
  }

  createAgentDefinition(draft: AgentDefinitionDraftRequest) {
    return this.request<AgentDefinitionResourceSummary>("/api/agent-definition-resources", {
      method: "POST",
      body: JSON.stringify(draft),
    });
  }

  saveAgentDefinitionDraft(definitionId: string, draft: AgentDefinitionDraftRequest) {
    return this.request<AgentDefinitionResourceSummary>(
      `/api/agent-definition-resources/${encodeURIComponent(definitionId)}/draft`,
      { method: "PUT", body: JSON.stringify(draft) },
    );
  }

  validateAgentDefinitionDraft(definitionId: string) {
    return this.request<AgentDefinitionValidationResult>(
      `/api/agent-definition-resources/${encodeURIComponent(definitionId)}/validate`,
      { method: "POST" },
    );
  }

  publishAgentDefinitionDraft(definitionId: string) {
    return this.request<AgentDefinitionReleaseSummary>(
      `/api/agent-definition-resources/${encodeURIComponent(definitionId)}/publish`,
      { method: "POST" },
    );
  }

  listSupervisorDefinitions() {
    return this.request<SupervisorDefinitionSummary[]>("/api/supervisor-definitions");
  }

  createSupervisorDefinition(draft: SupervisorDraftRequest) {
    return this.request<SupervisorDefinitionSummary>("/api/supervisor-definitions", {
      method: "POST",
      body: JSON.stringify(draft),
    });
  }

  saveSupervisorDraft(
    definitionId: string,
    draft: SupervisorDraftRequest,
    expectedRevision: number,
  ) {
    return this.request<SupervisorDefinitionSummary>(
      `/api/supervisor-definitions/${encodeURIComponent(definitionId)}/draft`,
      {
        method: "PUT",
        body: JSON.stringify({ draft, expected_revision: expectedRevision }),
      },
    );
  }

  validateSupervisorDraft(definitionId: string) {
    return this.request<SupervisorValidationResult>(
      `/api/supervisor-definitions/${encodeURIComponent(definitionId)}/validate`,
      { method: "POST" },
    );
  }

  publishSupervisorDraft(definitionId: string, expectedRevision: number) {
    return this.request<SupervisorReleaseSummary>(
      `/api/supervisor-definitions/${encodeURIComponent(definitionId)}/publish`,
      {
        method: "POST",
        body: JSON.stringify({ expected_revision: expectedRevision }),
      },
    );
  }

  getRunSupervisorPolicy(runId: string) {
    return this.request<SupervisorPolicyBinding | null>(
      `/api/runs/${encodeURIComponent(runId)}/supervisor-policy`,
    );
  }

  listRunAgents(runId: string) {
    return this.request<RuntimeAgentProjection[]>(
      `/api/runs/${encodeURIComponent(runId)}/agents`,
    );
  }

  listRunAgentActivities(runId: string) {
    return this.request<RuntimeAgentActivity[]>(
      `/api/runs/${encodeURIComponent(runId)}/agent-activities`,
    );
  }

  listRunAgentExecutions(runId: string) {
    return this.request<RuntimeAgentExecution[]>(
      `/api/runs/${encodeURIComponent(runId)}/agent-executions`,
    );
  }

  getRunCollaborationStatus(runId: string) {
    return this.request<CollaborationStatusSummary>(
      `/api/runs/${encodeURIComponent(runId)}/collaboration-status`,
    );
  }

  listRunProviderMetrics(runId: string) {
    return this.request<ProviderCallMetric[]>(
      `/api/runs/${encodeURIComponent(runId)}/provider-metrics`,
    );
  }

  listRunUserInputRequests(runId: string) {
    return this.request<PendingUserInputSummary[]>(
      `/api/runs/${encodeURIComponent(runId)}/user-input-requests`,
    );
  }

  listTaskArtifacts(taskId: string) {
    return this.request<ArtifactSummary[]>(
      `/api/tasks/${encodeURIComponent(taskId)}/artifacts`,
    );
  }

  getArtifact(artifactId: string) {
    return this.request<ArtifactSummary>(
      `/api/artifacts/${encodeURIComponent(artifactId)}`,
    );
  }

  cancelRun(runId: string) {
    return this.request<Run>(`/api/runs/${encodeURIComponent(runId)}/cancel`, {
      method: "POST",
    });
  }

  sendMessage(
    taskId: string,
    text: string,
    options: {
      model?: string | null;
      modelProvider?: string | null;
      effort?: string | null;
      serviceTier?: string | null;
      accessMode?: string | null;
      images?: string[];
      sourceAssetIds?: string[];
      collaborationMode?: Record<string, unknown> | null;
    } = {},
  ) {
    return this.request<{
      status: string;
      thread_id: string;
      turn_id: string;
      thread_name?: string | null;
    }>(
      `/api/tasks/${encodeURIComponent(taskId)}/messages`,
      {
        method: "POST",
        body: JSON.stringify({
          text,
          model: options.model ?? null,
          model_provider: options.modelProvider ?? null,
          effort: options.effort ?? null,
          service_tier: options.serviceTier ?? null,
          access_mode: options.accessMode ?? null,
          images: options.images ?? [],
          source_asset_ids: options.sourceAssetIds ?? [],
          collaboration_mode: options.collaborationMode ?? null,
        }),
      },
    );
  }

  listWorkspaceSourceAssets(workspaceId: string) {
    return this.request<SourceAssetSummary[]>(
      "/api/workspaces/" + encodeURIComponent(workspaceId) + "/source-assets",
    );
  }

  respondToDataIntake(taskId: string, request: DataIntakeResponseRequest) {
    return this.request<DataIntakeSessionSummary>(
      "/api/tasks/" + encodeURIComponent(taskId) + "/data-intake/responses",
      { method: "POST", body: JSON.stringify(request) },
    );
  }

  startAnalysis(taskId: string, request: AnalysisStartRequest) {
    return this.request<AnalysisStartResponse>(
      "/api/tasks/" + encodeURIComponent(taskId) + "/analysis-start",
      { method: "POST", body: JSON.stringify(request) },
    );
  }

  interruptRun(runId: string, turnId: string) {
    return this.request<{ status: string }>(`/api/runs/${encodeURIComponent(runId)}/interrupt`, {
      method: "POST",
      body: JSON.stringify({ turn_id: turnId }),
    });
  }

  steerRun(runId: string, turnId: string, text: string, images: string[] = []) {
    return this.request<{ status: string }>(`/api/runs/${encodeURIComponent(runId)}/steer`, {
      method: "POST",
      body: JSON.stringify({ turn_id: turnId, text, images }),
    });
  }

  compactRunThread(runId: string) {
    return this.request<{ status: string }>(`/api/runs/${encodeURIComponent(runId)}/compact`, {
      method: "POST",
    });
  }

  startRunReview(
    runId: string,
    target: Record<string, unknown>,
    delivery?: "inline" | "detached" | null,
  ) {
    return this.request<Record<string, unknown>>(
      `/api/runs/${encodeURIComponent(runId)}/review`,
      {
        method: "POST",
        body: JSON.stringify({ target, delivery: delivery ?? "inline" }),
      },
    );
  }

  listEvents(taskId: string, afterSequence?: number, limit = 200) {
    const query = new URLSearchParams({ limit: String(Math.min(200, Math.max(1, limit))) });
    if (afterSequence != null) query.set("after_sequence", String(afterSequence));
    return this.request<RunEvent[]>(
      `/api/tasks/${encodeURIComponent(taskId)}/events?${query.toString()}`,
    );
  }

  async latestEventSequence(taskId: string) {
    const events = await this.listEvents(taskId, undefined, 1);
    return events[events.length - 1]?.sequence ?? 0;
  }

  async listAllEvents(taskId: string, afterSequence = 0) {
    const events: RunEvent[] = [];
    let cursor = afterSequence;
    let hasAnotherPage = true;
    while (hasAnotherPage) {
      const page = await this.listEvents(taskId, cursor, 200);
      events.push(...page);
      hasAnotherPage = page.length === 200;
      if (!hasAnotherPage) break;
      const next = page[page.length - 1]?.sequence;
      if (typeof next !== "number" || next <= cursor) {
        throw new Error("Task event replay did not advance its sequence cursor");
      }
      cursor = next;
    }
    return events;
  }

  listApprovals() {
    return this.request<Approval[]>("/api/approvals");
  }

  decideApproval(id: string, decision: "accept" | "decline", version: number) {
    return this.request<void>(`/api/approvals/${encodeURIComponent(id)}/decision`, {
      method: "POST",
      body: JSON.stringify({ decision, version }),
    });
  }

  respondUserInput(
    id: string,
    answers: Record<string, { answers: string[] }>,
    version: number,
  ) {
    return this.request<void>(`/api/approvals/${encodeURIComponent(id)}/user-input`, {
      method: "POST",
      body: JSON.stringify({ answers, version }),
    });
  }

  workspaceStatus(workspaceId: string) {
    return this.request<WorkspaceStatus>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/status`,
    );
  }

  listWorkspaceGitRoots(workspaceId: string, depth: number) {
    const query = new URLSearchParams({ depth: String(depth) });
    return this.request<string[]>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/git-roots?${query.toString()}`,
    );
  }

  setWorkspaceGitRoot(workspaceId: string, gitRoot: string | null) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/git-roots`,
      { method: "PUT", body: JSON.stringify({ gitRoot }) },
    );
  }

  listWorkspaceFiles(workspaceId: string) {
    return this.request<string[]>(`/api/workspaces/${encodeURIComponent(workspaceId)}/files`);
  }

  uploadWorkspaceFiles(workspaceId: string, files: File[]) {
    if (files.length === 0) {
      throw new Error("Choose at least one file to upload.");
    }
    const body = new FormData();
    for (const file of files) {
      const relativePath = file.webkitRelativePath || file.name;
      body.append("files", file, relativePath);
    }
    return this.request<WorkspaceFileUploadResponse>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/files`,
      { method: "POST", body },
    );
  }

  listWorkspaceDatasetReleases(workspaceId: string) {
    return this.request<WorkspaceDatasetReleaseSummary[]>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/datasets`,
    );
  }

  publishWorkspaceDatasetRelease(
    workspaceId: string,
    request: PublishWorkspaceDatasetRequest,
    files: Map<string, File>,
  ) {
    const form = new FormData();
    form.append("metadata", JSON.stringify(request));
    for (const descriptor of request.files) {
      const file = files.get(descriptor.field_id);
      if (!file) {
        throw new Error(`Dataset file ${descriptor.logical_name} is missing.`);
      }
      form.append(descriptor.field_id, file, file.name);
    }
    return this.request<WorkspaceDatasetReleaseSummary>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/datasets`,
      { method: "POST", body: form },
    );
  }

  readWorkspaceFile(workspaceId: string, path: string) {
    const query = new URLSearchParams({ path });
    return this.request<WorkspaceFileContent>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/files/content?${query.toString()}`,
    );
  }

  async downloadWorkspaceFile(workspaceId: string, path: string) {
    const query = new URLSearchParams({ path });
    const response = await fetch(
      `${this.baseUrl}/api/workspaces/${encodeURIComponent(workspaceId)}/files/download?${query.toString()}`,
      {
        cache: "no-store",
        headers: this.token ? { authorization: `Bearer ${this.token}` } : undefined,
      },
    );
    if (!response.ok) {
      const text = await response.text();
      let message = `Request failed (HTTP ${response.status}).`;
      try {
        const payload = JSON.parse(text) as { message?: unknown };
        if (typeof payload.message === "string") message = payload.message;
      } catch {
        // Keep the typed fallback for non-JSON platform errors.
      }
      throw new Error(message);
    }
    const contentDisposition = response.headers.get("content-disposition") ?? "";
    const encodedFilename = contentDisposition.match(/filename\*=UTF-8''([^;]+)/i)?.[1];
    const quotedFilename = contentDisposition.match(/filename="([^"]+)"/i)?.[1];
    let filename = quotedFilename || path.split("/").pop() || "download";
    if (encodedFilename) {
      try {
        filename = decodeURIComponent(encodedFilename);
      } catch {
        // Use the ASCII fallback or requested path name when decoding fails.
      }
    }
    return { blob: await response.blob(), filename };
  }

  deleteWorkspaceFile(workspaceId: string, path: string) {
    return this.request<{ status: string; path: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/files`,
      { method: "DELETE", body: JSON.stringify({ path }) },
    );
  }

  writeWorkspaceAgents(workspaceId: string, content: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/agents`,
      { method: "PUT", body: JSON.stringify({ content }) },
    );
  }

  workspaceDiffs(workspaceId: string) {
    return this.request<WorkspaceFileDiff[]>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/diffs`,
    );
  }

  stageWorkspacePaths(workspaceId: string, paths: string[]) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/stage`,
      { method: "POST", body: JSON.stringify({ paths }) },
    );
  }

  stageAllWorkspacePaths(workspaceId: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/stage-all`,
      { method: "POST" },
    );
  }

  unstageWorkspacePaths(workspaceId: string, paths: string[]) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/unstage`,
      { method: "POST", body: JSON.stringify({ paths }) },
    );
  }

  revertWorkspacePaths(workspaceId: string, paths: string[]) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/revert`,
      { method: "POST", body: JSON.stringify({ paths }) },
    );
  }

  revertAllWorkspacePaths(workspaceId: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/revert-all`,
      { method: "POST" },
    );
  }

  listWorkspaceBranches(workspaceId: string) {
    return this.request<WorkspaceBranch[]>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/branches`,
    );
  }

  checkoutWorkspaceBranch(workspaceId: string, name: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/branches/checkout`,
      { method: "POST", body: JSON.stringify({ name }) },
    );
  }

  createWorkspaceBranch(workspaceId: string, name: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/branches`,
      { method: "POST", body: JSON.stringify({ name }) },
    );
  }

  renameWorkspaceBranch(workspaceId: string, name: string) {
    return this.request<{ status: string; name: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/branch/rename`,
      { method: "POST", body: JSON.stringify({ name }) },
    );
  }

  applyWorkspaceChanges(workspaceId: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/apply`,
      { method: "POST" },
    );
  }

  renameWorkspaceUpstream(workspaceId: string, oldBranch: string, newBranch: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/upstream/rename`,
      {
        method: "POST",
        body: JSON.stringify({ old_branch: oldBranch, new_branch: newBranch }),
      },
    );
  }

  openTerminal(workspaceId: string, terminalId: string, cols: number, rows: number) {
    return this.request<{ id: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/terminals`,
      {
        method: "POST",
        body: JSON.stringify({ terminal_id: terminalId, cols, rows }),
      },
    );
  }

  writeTerminal(workspaceId: string, terminalId: string, data: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/terminals/${encodeURIComponent(terminalId)}/write`,
      { method: "POST", body: JSON.stringify({ data }) },
    );
  }

  resizeTerminal(workspaceId: string, terminalId: string, cols: number, rows: number) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/terminals/${encodeURIComponent(terminalId)}/resize`,
      { method: "POST", body: JSON.stringify({ cols, rows }) },
    );
  }

  closeTerminal(workspaceId: string, terminalId: string) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/terminals/${encodeURIComponent(terminalId)}`,
      { method: "DELETE" },
    );
  }

  workspaceLog(workspaceId: string, limit = 40) {
    const query = new URLSearchParams({ limit: String(limit) });
    return this.request<WorkspaceLog>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/log?${query.toString()}`,
    );
  }

  workspaceCommitDiffs(workspaceId: string, sha: string) {
    return this.request<WorkspaceCommitDiff[]>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/commits/${encodeURIComponent(sha)}/diff`,
    );
  }

  githubIssues(workspaceId: string) {
    return this.request<GitHubIssues>(`/api/workspaces/${encodeURIComponent(workspaceId)}/github/issues`);
  }

  createGithubRepository(
    workspaceId: string,
    repo: string,
    visibility: "private" | "public",
    branch?: string | null,
  ) {
    return this.request<CreateGitHubRepositoryResponse>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/github/repository`,
      {
        method: "POST",
        body: JSON.stringify({ repo, visibility, branch: branch ?? null }),
      },
    );
  }

  githubPullRequests(workspaceId: string) {
    return this.request<GitHubPullRequests>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/github/pull-requests`,
    );
  }

  githubPullRequestDiff(workspaceId: string, number: number) {
    return this.request<GitHubPullRequestDiff[]>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/github/pull-requests/${number}/diff`,
    );
  }

  githubPullRequestComments(workspaceId: string, number: number) {
    return this.request<GitHubPullRequestComment[]>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/github/pull-requests/${number}/comments`,
    );
  }

  checkoutGithubPullRequest(workspaceId: string, number: number) {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/github/pull-requests/${number}/checkout`,
      { method: "POST" },
    );
  }

  workspaceRemote(workspaceId: string) {
    return this.request<string | null>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/remote`,
    );
  }

  workspaceRemoteOperation(workspaceId: string, operation: "fetch" | "pull" | "push" | "sync") {
    return this.request<{ status: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/${operation}`,
      { method: "POST" },
    );
  }

  profileAccount() {
    return this.request<{ data: Record<string, unknown> }>("/api/profile/account")
      .then((response) => response.data);
  }

  startProfileLogin() {
    return this.request<ProfileLoginStart>("/api/profile/login", { method: "POST" });
  }

  cancelProfileLogin() {
    return this.request<ProfileLoginCancel>("/api/profile/login", { method: "DELETE" });
  }

  profileLoginStatus(loginId: string) {
    return this.request<ProfileLoginStatus>(
      `/api/profile/login/${encodeURIComponent(loginId)}`,
    );
  }

  profileRateLimits() {
    return this.request<{ data: Record<string, unknown> }>("/api/profile/rate-limits")
      .then((response) => response.data);
  }

  profileUsage(days: number, workspaceId?: string | null) {
    const query = new URLSearchParams({ days: String(days) });
    if (workspaceId) query.set("workspaceId", workspaceId);
    return this.request<{
      updatedAt: number;
      days: Array<{ day: string; inputTokens: number; cachedInputTokens: number; outputTokens: number; totalTokens: number; agentTimeMs: number; agentRuns: number }>;
      totals: { last7DaysTokens: number; last30DaysTokens: number; averageDailyTokens: number; cacheHitRatePercent: number; peakDay: string | null; peakDayTokens: number };
      topModels: Array<{ model: string; tokens: number; sharePercent: number }>;
    }>(`/api/profile/usage?${query.toString()}`);
  }

  profileCollaborationModes() {
    return this.request<{ data: Record<string, unknown> }>("/api/profile/collaboration-modes")
      .then((response) => response.data);
  }

  profileSkills(runId: string, forceReload = false) {
    const query = new URLSearchParams({ runId, forceReload: String(forceReload) });
    return this.request<{ data: Record<string, unknown> }>(
      `/api/profile/skills?${query.toString()}`,
    ).then((response) => response.data);
  }

  profileApps(runId: string, cursor?: string | null, limit?: number | null) {
    return this.profileList("apps", runId, cursor, limit);
  }

  profileRuntimeStatus() {
    return this.request<{ data: Record<string, unknown> }>("/api/profile/runtime-status")
      .then((response) => response.data);
  }

  profileMcpServers(runId: string, cursor?: string | null, limit?: number | null) {
    return this.profileList("mcp-servers", runId, cursor, limit);
  }

  profileExperimentalFeatures(runId: string, cursor?: string | null, limit?: number | null) {
    return this.profileList("experimental-features", runId, cursor, limit);
  }

  setExperimentalFeature(name: string, enabled: boolean) {
    return this.request<{ status: string }>(
      `/api/profile/features/${encodeURIComponent(name)}`,
      { method: "PUT", body: JSON.stringify({ enabled }) },
    );
  }

  readProfileFile(kind: "agents" | "config") {
    return this.request<ProfileTextFile>(`/api/profile/files/${kind}`);
  }

  writeProfileFile(kind: "agents" | "config", content: string) {
    return this.request<{ status: string }>(`/api/profile/files/${kind}`, {
      method: "PUT",
      body: JSON.stringify({ content }),
    });
  }

  getAgents() {
    return this.request<AgentsSettings>("/api/profile/agents");
  }

  getConfigModel() {
    return this.request<{ model: string | null }>("/api/profile/config/model");
  }

  setAgentsCore(input: {
    multiAgentEnabled: boolean;
    maxThreads: number;
    maxDepth: number;
  }) {
    return this.request<AgentsSettings>("/api/profile/agents/settings", {
      method: "PUT",
      body: JSON.stringify(input),
    });
  }

  createAgent(input: Record<string, unknown>) {
    return this.request<AgentsSettings>("/api/profile/agents", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  updateAgent(originalName: string, input: Record<string, unknown>) {
    return this.request<AgentsSettings>(`/api/profile/agents/${encodeURIComponent(originalName)}`, {
      method: "PATCH",
      body: JSON.stringify(input),
    });
  }

  deleteAgent(name: string, deleteManagedFile: boolean) {
    const query = new URLSearchParams({ deleteManagedFile: String(deleteManagedFile) });
    return this.request<AgentsSettings>(
      `/api/profile/agents/${encodeURIComponent(name)}?${query.toString()}`,
      { method: "DELETE" },
    );
  }

  readAgentConfig(name: string) {
    return this.request<string>(`/api/profile/agents/${encodeURIComponent(name)}/config`);
  }

  writeAgentConfig(name: string, content: string) {
    return this.request<{ status: string }>(
      `/api/profile/agents/${encodeURIComponent(name)}/config`,
      { method: "PUT", body: JSON.stringify({ content }) },
    );
  }

  listPrompts(runId: string) {
    const query = new URLSearchParams({ runId });
    return this.request<PromptEntry[]>(`/api/profile/prompts?${query.toString()}`);
  }

  createPrompt(input: Record<string, unknown>) {
    return this.request<PromptEntry>("/api/profile/prompts", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  updatePrompt(input: Record<string, unknown>) {
    return this.request<PromptEntry>("/api/profile/prompts", {
      method: "PUT",
      body: JSON.stringify(input),
    });
  }

  deletePrompt(input: Record<string, unknown>) {
    return this.request<{ status: string }>("/api/profile/prompts", {
      method: "DELETE",
      body: JSON.stringify(input),
    });
  }

  movePrompt(input: Record<string, unknown>) {
    return this.request<PromptEntry>("/api/profile/prompts/move", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  generateText(runId: string, kind: "runMetadata" | "agentDescription" | "commitMessage", input: string, model?: string | null) {
    return this.request<{ text: string }>(`/api/runs/${encodeURIComponent(runId)}/generate`, {
      method: "POST",
      body: JSON.stringify({ kind, input, model: model ?? null }),
    });
  }

  rememberApprovalRule(runId: string, command: string[]) {
    return this.request<{ ok: boolean }>("/api/profile/approval-rules", {
      method: "POST",
      body: JSON.stringify({ runId, command }),
    });
  }

  private profileList(
    resource: "apps" | "mcp-servers" | "experimental-features",
    runId: string,
    cursor?: string | null,
    limit?: number | null,
  ) {
    const query = new URLSearchParams({ runId });
    if (cursor) query.set("cursor", cursor);
    if (limit != null) query.set("limit", String(limit));
    return this.request<{ data: Record<string, unknown> }>(
      `/api/profile/${resource}?${query.toString()}`,
    ).then((response) => response.data);
  }

  commitWorkspace(workspaceId: string, selectedPaths: string[], message: string) {
    return this.request<{ workspace_id: string; commit: string }>(
      `/api/workspaces/${encodeURIComponent(workspaceId)}/commit`,
      {
        method: "POST",
        body: JSON.stringify({ selected_paths: selectedPaths, message }),
      },
    );
  }

  listProviders() {
    return this.request<ProviderCatalog>("/api/providers");
  }

  selectProvider(id: string) {
    return this.request<ProviderCatalog>(`/api/providers/${encodeURIComponent(id)}/select`, {
      method: "POST",
    });
  }

  selectProviderModel(providerId: string, modelId: string) {
    return this.request<ProviderCatalog>(
      `/api/providers/${encodeURIComponent(providerId)}/models/${encodeURIComponent(modelId)}/select`,
      { method: "POST" },
    );
  }

  upsertProvider(id: string, input: Record<string, unknown>) {
    return this.request<ProviderCatalog>(`/api/providers/${encodeURIComponent(id)}`, {
      method: "PUT",
      body: JSON.stringify(input),
    });
  }

  deleteProvider(id: string) {
    return this.request<ProviderCatalog>(`/api/providers/${encodeURIComponent(id)}`, {
      method: "DELETE",
    });
  }

  refreshProviderModels(id: string) {
    return this.request<ProviderCatalog>(
      `/api/providers/${encodeURIComponent(id)}/models/refresh`,
      { method: "POST" },
    );
  }

  updateProviderModel(providerId: string, modelId: string, contextWindow: number) {
    return this.request<ProviderCatalog>(
      `/api/providers/${encodeURIComponent(providerId)}/models/${encodeURIComponent(modelId)}`,
      { method: "PATCH", body: JSON.stringify({ contextWindow }) },
    );
  }

  subscribe(
    onEvent: (event: RunEvent) => void,
    onState: (state: "connecting" | "online" | "offline" | "resync") => void,
  ) {
    let disposed = false;
    let socket: WebSocket | null = null;
    let retry: number | null = null;
    let attempts = 0;
    const connect = () => {
      if (disposed || !this.token) return;
      onState("connecting");
      const endpoint = new URL(`${this.baseUrl || window.location.origin}/api/events/ws`);
      endpoint.protocol = endpoint.protocol === "https:" ? "wss:" : "ws:";
      socket = new WebSocket(endpoint);
      socket.onopen = () => {
        socket?.send(JSON.stringify({ type: "authenticate", token: this.token }));
      };
      socket.onmessage = (message) => {
        const envelope = JSON.parse(String(message.data)) as LiveEnvelope;
        if (envelope.type === "ready") {
          attempts = 0;
          onState("online");
        } else if (envelope.type === "run.event") {
          onEvent(envelope.event);
        } else if (envelope.type === "resyncRequired") {
          onState("resync");
        }
      };
      socket.onclose = () => {
        if (disposed) return;
        onState("offline");
        const delay = Math.min(10_000, 500 * 2 ** attempts++);
        retry = window.setTimeout(connect, delay);
      };
      socket.onerror = () => socket?.close();
    };
    connect();
    return () => {
      disposed = true;
      if (retry != null) window.clearTimeout(retry);
      socket?.close();
    };
  }
}
