export type PlatformHealth = {
  ok: boolean;
  version: string;
  started_at?: string;
  uptime_seconds?: number;
};

export type PlatformUser = {
  id: string;
  name: string;
  email: string;
  role: string;
};

export type PlatformProject = {
  id: string;
  organization_id: string;
  name: string;
  git_url: string;
  default_branch: string;
  created_at: string;
  updated_at: string;
};

export type PlatformTask = {
  id: string;
  project_id: string;
  title: string;
  status: string;
  created_at: string;
  updated_at: string;
};

export type PlatformRun = {
  id: string;
  task_id: string;
  status: string;
  codex_thread_id: string | null;
  created_at: string;
  updated_at: string;
};

export type PlatformApproval = {
  id: string;
  run_id: string;
  request_type: string;
  request_payload: Record<string, unknown>;
  status: string;
  codex_request_id: string | null;
  workspace_id: string | null;
  thread_id: string | null;
  decision: string | null;
  decided_by: string | null;
  decided_at: string | null;
  created_at: string;
  expires_at: string | null;
};

export type PlatformRunEvent = {
  id: string;
  sequence: number;
  run_id: string;
  event_type: string;
  projection_version: number;
  thread_id: string | null;
  turn_id: string | null;
  item_id: string | null;
  payload: {
    schemaVersion: number;
    threadId: string;
    turnId?: string | null;
    itemId?: string | null;
    lifecycle: string;
    itemType?: string | null;
    data: Record<string, unknown>;
  };
  created_at: string;
};

export type GitStatusFile = {
  path: string;
  status: string;
};
