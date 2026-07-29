export type User = {
  id: string;
  name: string;
  username: string;
  email: string;
  role: string;
};

export type Organization = {
  id: string;
  name: string;
  slug: string;
};

export type Session = {
  user: User;
  organization: Organization;
  session_token: string;
  membership_role?: string;
};

export type Me = User & {
  organization_id: string;
  organization_role: string;
};

export type Project = {
  id: string;
  name: string;
  git_url: string;
  default_branch: string;
  created_at: string;
  updated_at: string;
};

export type Task = {
  id: string;
  project_id: string;
  title: string;
  status: string;
  model_provider: string | null;
  model: string | null;
  created_at: string;
  updated_at: string;
};

export type Run = {
  id: string;
  task_id: string;
  status: string;
  codex_thread_id: string | null;
  active_turn_id: string | null;
  workspace_id: string | null;
  attempt: number;
  created_at: string;
  updated_at: string;
};

export type SupervisorPolicySelection = {
  policy_id: string;
  version: string;
};

export type SupervisorPolicySummary = SupervisorPolicySelection & {
  display_name: string;
  description: string;
  source: "repository" | "user_release";
};

export type SupervisorInstructionPolicySelection = {
  policy_id: string;
  version: string;
};

export type SupervisorInstructionPolicySummary = SupervisorInstructionPolicySelection & {
  release_id: string | null;
  display_name: string;
  description: string;
  source: "repository" | "platform_release";
  content_sha256: string;
};

export type SupervisorInstructionPolicyDetail = SupervisorInstructionPolicySummary & {
  platform_instructions: string;
};

export type SupervisorInstructionPolicyPublishRequest = SupervisorInstructionPolicySelection & {
  display_name: string;
  description: string;
  platform_instructions: string;
};

export type SupervisorPolicyBinding = SupervisorPolicySelection & {
  run_id: string;
  task_id: string;
  thread_id: string | null;
  display_name: string;
  content_sha256: string;
  state: "prepared" | "bound" | "failed" | "cancelled";
  created_at: string;
  bound_at: string | null;
};

export type AgentDefinitionSummary = {
  source: "repository" | "user_release";
  release_id: string | null;
  definition_id: string;
  version: string;
  display_name: string;
  description: string;
  responsibilities: string[];
  input_artifact_types: string[];
  output_artifact_types: string[];
  required_capabilities: string[];
  capability_template: AgentCapabilityTemplateSelection | null;
};

export type AgentDefinitionDetail = AgentDefinitionSummary & {
  developer_instructions: string;
  content_sha256: string;
  execution_semantics_sha256: string;
};

export type CapabilityPackageSummary = {
  package_id: string;
  version: string;
  display_name: string;
  description: string;
  capability_root_id: string;
  capabilities: string[];
  mcp_server_names: string[];
  includes_skills: boolean;
  source: "repository";
};

export type PythonCapabilityTool = {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
};

export type PythonCapabilitySkill = {
  name: string;
  description: string;
  instructions: string;
};

export type PythonCapabilityPublishRequest = {
  slug: string;
  version: string;
  display_name: string;
  description: string;
  server_name: string;
  python_source: string;
  tools: PythonCapabilityTool[];
  skill: PythonCapabilitySkill;
};

export type PythonCapabilityValidationIssue = {
  code: string;
  message: string;
};

export type PythonCapabilityValidationResult = {
  valid: boolean;
  tool_names: string[];
  issues: PythonCapabilityValidationIssue[];
};

export type PythonCapabilityToolTestRequest = {
  capability: PythonCapabilityPublishRequest;
  tool_name: string;
  arguments: Record<string, unknown>;
};

export type PythonCapabilityToolTestResponse = {
  tool_name: string;
  result: unknown;
};

export type PythonCapabilityPublishResponse = {
  package_id: string;
  version: string;
  capability_root_id: string;
  server_name: string;
  skill_name: string;
  written_files: string[];
};

export type AgentCapabilityTemplateSelection = {
  definition_id: string;
  version: string;
};

export type AgentDefinitionDraftRequest = {
  definition_id: string;
  version: string;
  display_name: string;
  description: string;
  responsibilities: string[];
  developer_instructions: string;
  input_artifact_types: string[];
  output_artifact_types: string[];
  capability_template: AgentCapabilityTemplateSelection;
};

export type AgentDefinitionValidationIssue = {
  code: string;
  message: string;
};

export type AgentDefinitionValidationResult = {
  valid: boolean;
  content_sha256: string | null;
  execution_semantics_sha256: string | null;
  issues: AgentDefinitionValidationIssue[];
};

export type AgentDefinitionReleaseSummary = {
  id: string;
  definition_id: string;
  version: string;
  display_name: string;
  description: string;
  content_sha256: string;
  published_at: string;
};

export type AgentDefinitionResourceSummary = {
  id: string;
  definition_id: string;
  display_name: string;
  description: string;
  owner_user_id: string;
  draft: AgentDefinitionDraftRequest | null;
  releases: AgentDefinitionReleaseSummary[];
  created_at: string;
  updated_at: string;
};

export type SupervisorAgentSelection = {
  definition_id: string;
  version: string;
  release_id: string | null;
  spawn_limit: number;
};

export type SupervisorArtifactContractInput = {
  artifact_type: string;
  producer_agent: string;
  consumer_agents: string[];
  required: boolean;
};

export type SupervisorPolicyDetail = SupervisorPolicySummary & {
  responsibilities: string[];
  instruction_policy: SupervisorInstructionPolicySummary;
  platform_instructions: string;
  custom_instructions: string;
  agents: SupervisorAgentSelection[];
  artifact_contracts: SupervisorArtifactContractInput[];
  max_active_child_agents: number;
  content_sha256: string;
  execution_semantics_sha256: string;
};

export type SupervisorDraftRequest = {
  policy_id: string;
  version: string;
  display_name: string;
  description: string;
  responsibilities: string[];
  instruction_policy: SupervisorInstructionPolicySelection;
  custom_instructions: string;
  agents: SupervisorAgentSelection[];
  artifact_contracts: SupervisorArtifactContractInput[];
  max_active_child_agents: number;
};

export type SupervisorValidationResult = {
  valid: boolean;
  content_sha256: string | null;
  execution_semantics_sha256: string | null;
  issues: Array<{ code: string; message: string }>;
};

export type SupervisorReleaseSummary = Omit<SupervisorPolicySummary, "source"> & {
  id: string;
  content_sha256: string;
  published_at: string;
};

export type SupervisorDefinitionSummary = {
  id: string;
  policy_id: string;
  display_name: string;
  description: string;
  owner_user_id: string;
  draft: SupervisorDraftRequest | null;
  releases: SupervisorReleaseSummary[];
  created_at: string;
  updated_at: string;
};

export type RuntimeAgentProjection = {
  run_id: string;
  thread_id: string;
  parent_thread_id: string | null;
  source_kind: string;
  agent_path: string | null;
  agent_nickname: string | null;
  agent_role: string | null;
  status_type: string | null;
  active_flags: string[];
  is_root: boolean;
  first_observed_at: string;
  last_observed_at: string;
};

export type RuntimeAgentActivityKind =
  | "assignment"
  | "guidance"
  | "turn_started"
  | "turn_completed"
  | "tool_started"
  | "tool_completed"
  | "tool_failed"
  | "reporting"
  | "waiting"
  | "completed"
  | "failed"
  | "interrupted";

export type RuntimeAgentActivity = {
  run_id: string;
  sequence: number;
  thread_id: string;
  turn_id: string | null;
  item_id: string | null;
  kind: RuntimeAgentActivityKind;
  status: "pending" | "running" | "completed" | "failed" | "waiting";
  title: string;
  detail: string | null;
  created_at: string;
};

export type RuntimeAgentExecution = {
  id: string;
  run_id: string;
  thread_id: string;
  turn_id: string | null;
  ordinal: number;
  task: string | null;
  status: "pending" | "running" | "waiting" | "completed" | "failed" | "interrupted";
  current_behavior: string;
  latest_progress: string | null;
  first_observed_sequence: number;
  last_observed_sequence: number;
  started_at: string | null;
  completed_at: string | null;
  created_at: string;
  updated_at: string;
};

export type ArtifactSummary = {
  id: string;
  task_id: string;
  artifact_schema: string;
  display_name: string;
  mime_type: string;
  expected_size: number | null;
  byte_size: number | null;
  content_sha256: string | null;
  state: "pending" | "materializing" | "ready" | "failed";
  producer_run_id: string;
  producer_thread_id: string;
  producer_turn_id: string;
  producer_item_id: string;
  producer_agent_role: string | null;
  created_at: string;
  updated_at: string;
};

export type Workspace = {
  id: string;
  project_id: string;
  name: string;
  kind: "main" | "worktree" | "clone";
  state: "creating" | "ready" | "retained" | "removing" | "removed" | "cleanup_failed";
  source_ref: string;
  branch_name: string | null;
  parent_workspace_id: string | null;
  group_workspace_id: string | null;
  managed: boolean;
  created_at: string;
  updated_at: string;
};

export type ProjectThreadContext = {
  project: Project;
  task: Task;
  run: Run;
};

export type ThreadHistoryTurn = {
  id: string;
  status: string;
  items: Record<string, unknown>[];
  error?: { message: string; additionalDetails?: string | null } | null;
  startedAt?: number | null;
  completedAt?: number | null;
  durationMs?: number | null;
};

export type ThreadHistoryResponse = {
  thread: {
    id: string;
    name?: string | null;
    preview: string;
    createdAt: number;
    updatedAt: number;
    status: { type: string; activeFlags?: string[] };
    turns: ThreadHistoryTurn[];
  };
};

export type RunEvent = {
  id: string;
  sequence: number;
  run_id: string;
  event_type: string;
  projection_version: number;
  thread_id: string | null;
  turn_id: string | null;
  item_id: string | null;
  payload: {
    schemaVersion?: number;
    lifecycle?: string;
    itemType?: string | null;
    data?: unknown;
  };
  created_at: string;
};

export type Approval = {
  id: string;
  runId: string;
  threadId: string;
  requestType: string;
  itemId?: string | null;
  reason?: string | null;
  command?: string | null;
  state: string;
  version: number;
  createdAt: string;
  decidedAt?: string | null;
};

export type WorkspaceChange = {
  path: string;
  status: string;
  additions: number | null;
  deletions: number | null;
  binary: boolean;
  size_bytes: number | null;
  large: boolean;
};

export type WorkspaceStatus = {
  workspace_id: string;
  branch: string;
  head_commit: string;
  changes: WorkspaceChange[];
};

export type WorkspaceFileContent = {
  content: string;
  truncated: boolean;
};

export type WorkspaceFileDiff = {
  path: string;
  diff: string;
  isBinary: boolean;
  truncated: boolean;
};

export type WorkspaceBranch = {
  name: string;
  lastCommit: number;
};

export type WorkspaceLogEntry = {
  sha: string;
  summary: string;
  author: string;
  timestamp: number;
};

export type WorkspaceLog = {
  total: number;
  entries: WorkspaceLogEntry[];
  ahead: number;
  behind: number;
  aheadEntries: WorkspaceLogEntry[];
  behindEntries: WorkspaceLogEntry[];
  upstream: string | null;
};

export type WorkspaceCommitDiff = {
  path: string;
  status: string;
  diff: string;
  isBinary: boolean;
};

export type ProviderModel = {
  modelId: string;
  modelName?: string | null;
  showInPicker: boolean;
  contextWindow?: number | null;
};

export type Provider = {
  id: string;
  name: string;
  wireApi: string;
  kind: "builtIn" | "local" | "custom";
  isCurrent: boolean;
  modelCount: number;
  models: ProviderModel[];
};

export type ProviderCatalog = {
  data: Provider[];
  currentProviderId: string;
  currentModelId?: string | null;
};

export type ModelSelection = {
  providerId: string;
  modelId: string;
};

export type ProfileTextFile = {
  exists: boolean;
  content: string;
  truncated: boolean;
};

export type ProfileLoginStart = { loginId: string; authUrl: string };
export type ProfileLoginCancel = { canceled: boolean; status: string };
export type ProfileLoginStatus = {
  completed: boolean;
  success: boolean | null;
  error: string | null;
};

export type BrowserWorkspacePreference = {
  workspaceId: string;
  settings: Record<string, unknown>;
  runtimeCodexArgs: string | null;
};

export type MapsProvider = "mapbox" | "google";

export type MapsConfiguration = {
  configured: boolean;
  provider: MapsProvider | null;
  mapboxAccessToken: string | null;
  canConfigure: boolean;
  updatedAt: string | null;
};

export type AgentSummary = {
  name: string;
  description: string | null;
  developerInstructions: string | null;
  configFile: string;
  resolvedPath: string;
  managedByApp: boolean;
  fileExists: boolean;
};

export type AgentsSettings = {
  configPath: string;
  multiAgentEnabled: boolean;
  maxThreads: number;
  maxDepth: number;
  agents: AgentSummary[];
};

export type PromptEntry = {
  name: string;
  path: string;
  description: string | null;
  argumentHint: string | null;
  content: string;
  scope: "workspace" | "global";
};

export type GitHubIssue = {
  number: number;
  title: string;
  url: string;
  updatedAt: string;
};

export type GitHubIssues = { total: number; issues: GitHubIssue[] };
export type GitHubUser = { login: string };

export type GitHubPullRequest = {
  number: number;
  title: string;
  url: string;
  updatedAt: string;
  createdAt: string;
  body: string;
  headRefName: string;
  baseRefName: string;
  isDraft: boolean;
  author: GitHubUser | null;
};

export type GitHubPullRequests = { total: number; pullRequests: GitHubPullRequest[] };
export type GitHubPullRequestDiff = { path: string; status: string; diff: string };
export type GitHubPullRequestComment = {
  id: number;
  body: string;
  createdAt: string;
  url: string;
  author: GitHubUser | null;
};

export type CreateGitHubRepositoryResponse = {
  status: "ok" | "partial";
  repo: string;
  remoteUrl: string | null;
  pushError?: string | null;
  defaultBranchError?: string | null;
};
