import type {
  GitStatusFile,
  PlatformApproval,
  PlatformHealth,
  PlatformProject,
  PlatformRun,
  PlatformRunEvent,
  PlatformTask,
  PlatformUser,
} from "./platformTypes";

type PlatformClientOptions = {
  baseUrl?: string;
  token?: string;
};

type PlatformErrorBody = {
  kind?: string;
  message?: string;
  error?: { message?: string; code?: string };
};

function platformErrorMessage(payload: PlatformErrorBody | null, status: number) {
  if (!payload) {
    return `Platform request failed (HTTP ${status}).`;
  }
  if (typeof payload.message === "string" && payload.message.trim()) {
    return payload.message;
  }
  if (typeof payload.error?.message === "string" && payload.error.message.trim()) {
    return payload.error.message;
  }
  return `Platform request failed (HTTP ${status}).`;
}

function defaultBaseUrl() {
  return import.meta.env.VITE_OPEN_WEB_CODEX_API ?? "http://127.0.0.1:4800";
}

export class PlatformClient {
  private baseUrl: string;
  private token: string;

  constructor(options: PlatformClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? defaultBaseUrl()).replace(/\/$/, "");
    this.token = options.token ?? "";
  }

  setToken(token: string) {
    this.token = token.trim();
  }

  setBaseUrl(baseUrl: string) {
    this.baseUrl = baseUrl.trim().replace(/\/$/, "");
  }

  private async fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        ...(init?.body ? { "content-type": "application/json" } : {}),
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
        ...init?.headers,
      },
    });
    const body = await response.text();
    let payload: T | PlatformErrorBody | null = null;
    if (body) {
      try {
        payload = JSON.parse(body) as T | PlatformErrorBody;
      } catch {
        throw new Error(`Platform API returned invalid JSON (HTTP ${response.status}).`);
      }
    }
    if (!response.ok) {
      const errorBody = payload as PlatformErrorBody | null;
      throw new Error(platformErrorMessage(errorBody, response.status));
    }
    return payload as T;
  }

  health() {
    return this.fetchJson<PlatformHealth>("/api/health");
  }

  bootstrap(name: string, email: string, password: string) {
    return this.fetchJson<{ session_token: string; user: PlatformUser }>("/api/bootstrap", {
      method: "POST",
      body: JSON.stringify({ name, email, password }),
    });
  }

  login(email: string, password: string) {
    return this.fetchJson<{ session_token: string; user: PlatformUser }>("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    });
  }

  me() {
    return this.fetchJson<PlatformUser>("/api/me");
  }

  listProjects() {
    return this.fetchJson<PlatformProject[]>("/api/projects");
  }

  createProject(input: { name: string; git_url: string; default_branch?: string }) {
    return this.fetchJson<PlatformProject>("/api/projects", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  getProject(projectId: string) {
    return this.fetchJson<PlatformProject>(`/api/projects/${encodeURIComponent(projectId)}`);
  }

  deleteProject(projectId: string) {
    return this.fetchJson<{ deleted: boolean; id: string }>(
      `/api/projects/${encodeURIComponent(projectId)}`,
      { method: "DELETE" },
    );
  }

  listTasks(projectId: string) {
    const query = new URLSearchParams({ project_id: projectId });
    return this.fetchJson<PlatformTask[]>(`/api/tasks?${query.toString()}`);
  }

  createTask(projectId: string, title: string) {
    return this.fetchJson<PlatformTask>("/api/tasks", {
      method: "POST",
      body: JSON.stringify({ project_id: projectId, title }),
    });
  }

  getTask(taskId: string) {
    return this.fetchJson<PlatformTask>(`/api/tasks/${encodeURIComponent(taskId)}`);
  }

  getActiveRun(taskId: string) {
    return this.fetchJson<{ run: PlatformRun | null }>(
      `/api/tasks/${encodeURIComponent(taskId)}/active-run`,
    );
  }

  startTaskRun(taskId: string, idempotencyKey?: string) {
    return this.fetchJson<{ run: PlatformRun }>(
      `/api/tasks/${encodeURIComponent(taskId)}/runs`,
      {
        method: "POST",
        body: "{}",
        headers: idempotencyKey ? { "idempotency-key": idempotencyKey } : {},
      },
    );
  }

  sendMessage(
    taskId: string,
    text: string,
    options?: { model?: string | null; effort?: string | null; accessMode?: string | null },
  ) {
    const body: Record<string, string> = { text };
    if (options?.model?.trim()) {
      body.model = options.model.trim();
    }
    if (options?.effort?.trim()) {
      body.effort = options.effort.trim();
    }
    if (options?.accessMode?.trim()) {
      body.access_mode = options.accessMode.trim();
    }
    return this.fetchJson<{ status: string; thread_id: string }>(
      `/api/tasks/${encodeURIComponent(taskId)}/messages`,
      {
        method: "POST",
        body: JSON.stringify(body),
      },
    );
  }

  updateThreadSettings(taskId: string, settings: { model?: string | null; effort?: string | null }) {
    const body: Record<string, string> = {};
    if (settings.model?.trim()) {
      body.model = settings.model.trim();
    }
    if (settings.effort?.trim()) {
      body.effort = settings.effort.trim();
    }
    return this.fetchJson<{ status: string; thread_id: string }>(
      `/api/tasks/${encodeURIComponent(taskId)}/thread-settings`,
      {
        method: "PATCH",
        body: JSON.stringify(body),
      },
    );
  }

  listModelProviders() {
    return this.fetchJson<Record<string, unknown>>("/api/codex/model-providers");
  }

  listModels(forceRefresh = false) {
    const query = forceRefresh ? "?force_refresh=true" : "";
    return this.fetchJson<Record<string, unknown>>(`/api/codex/models${query}`);
  }

  writeModelProvider(input: Record<string, unknown>) {
    return this.fetchJson<Record<string, unknown>>("/api/codex/model-providers/write", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  listTaskEvents(taskId: string, afterSequence?: number, limit = 200) {
    const query = new URLSearchParams({ limit: String(limit) });
    if (afterSequence != null) {
      query.set("after_sequence", String(afterSequence));
    }
    return this.fetchJson<PlatformRunEvent[]>(
      `/api/tasks/${encodeURIComponent(taskId)}/events?${query.toString()}`,
    );
  }

  listApprovals(runId?: string) {
    const query = runId ? `?run_id=${encodeURIComponent(runId)}` : "";
    return this.fetchJson<PlatformApproval[]>(`/api/approvals${query}`);
  }

  decideApproval(approvalId: string, decision: "approved" | "rejected") {
    return this.fetchJson<{ approval: PlatformApproval }>(
      `/api/approvals/${encodeURIComponent(approvalId)}/decision`,
      {
        method: "POST",
        body: JSON.stringify({ decision }),
      },
    );
  }

  getRun(runId: string) {
    return this.fetchJson<PlatformRun>(`/api/runs/${encodeURIComponent(runId)}`);
  }

  cancelRun(runId: string) {
    return this.fetchJson<PlatformRun>(`/api/runs/${encodeURIComponent(runId)}/cancel`, {
      method: "POST",
      body: "{}",
    });
  }

  interruptRun(runId: string, turnId: string) {
    return this.fetchJson<{ status: string }>(
      `/api/runs/${encodeURIComponent(runId)}/interrupt`,
      {
        method: "POST",
        body: JSON.stringify({ turn_id: turnId }),
      },
    );
  }

  steerRun(runId: string, turnId: string, text: string) {
    return this.fetchJson<{ status: string }>(
      `/api/runs/${encodeURIComponent(runId)}/steer`,
      {
        method: "POST",
        body: JSON.stringify({ turn_id: turnId, text }),
      },
    );
  }

  listRunFiles(runId: string) {
    return this.fetchJson<{ files: string[] }>(
      `/api/runs/${encodeURIComponent(runId)}/files`,
    );
  }

  readRunFile(runId: string, path: string) {
    const query = new URLSearchParams({ path });
    return this.fetchJson<{ path: string; content: string; truncated: boolean }>(
      `/api/runs/${encodeURIComponent(runId)}/files/content?${query.toString()}`,
    );
  }

  getRunGitStatus(runId: string) {
    return this.fetchJson<{ files: GitStatusFile[] }>(
      `/api/runs/${encodeURIComponent(runId)}/git-status`,
    );
  }
}
