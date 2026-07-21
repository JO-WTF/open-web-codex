export type User = {
  id: string;
  name: string;
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
  source_ref: string | null;
  attempt: number;
  created_at: string;
  updated_at: string;
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
};
