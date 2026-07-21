import type {
  Approval,
  Me,
  Project,
  ProviderCatalog,
  Run,
  RunEvent,
  Session,
  Task,
  WorkspaceStatus,
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

export class PlatformClient {
  private readonly baseUrl: string;
  private readonly token: string;

  constructor(options: ClientOptions = {}) {
    this.baseUrl = (options.baseUrl ?? defaultBaseUrl()).replace(/\/$/, "");
    this.token = options.token?.trim() ?? "";
  }

  private async request<T>(path: string, init?: RequestInit): Promise<T> {
    const response = await fetch(`${this.baseUrl}${path}`, {
      ...init,
      headers: {
        ...(init?.body ? { "content-type": "application/json" } : {}),
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
        ...init?.headers,
      },
    });
    const text = await response.text();
    const payload = text ? JSON.parse(text) as unknown : null;
    if (!response.ok) {
      const record = payload && typeof payload === "object"
        ? payload as Record<string, unknown>
        : null;
      const message = typeof record?.message === "string"
        ? record.message
        : `Request failed (HTTP ${response.status}).`;
      throw new Error(message);
    }
    return payload as T;
  }

  health() {
    return this.request<{ ok: boolean; version: string }>("/api/health");
  }

  bootstrap(name: string, email: string, password: string) {
    return this.request<Session>("/api/bootstrap", {
      method: "POST",
      body: JSON.stringify({ name, email, password }),
    });
  }

  login(email: string, password: string) {
    return this.request<Session>("/api/sessions", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    });
  }

  me() {
    return this.request<Me>("/api/me");
  }

  listProjects() {
    return this.request<Project[]>("/api/projects");
  }

  createProject(name: string, gitUrl: string, defaultBranch: string) {
    return this.request<Project>("/api/projects", {
      method: "POST",
      body: JSON.stringify({ name, git_url: gitUrl, default_branch: defaultBranch }),
    });
  }

  listTasks(projectId: string) {
    return this.request<Task[]>(`/api/tasks?project_id=${encodeURIComponent(projectId)}`);
  }

  createTask(projectId: string, title: string) {
    return this.request<Task>("/api/tasks", {
      method: "POST",
      body: JSON.stringify({ project_id: projectId, title }),
    });
  }

  listRuns(taskId: string) {
    return this.request<Run[]>(`/api/runs?task_id=${encodeURIComponent(taskId)}`);
  }

  getRun(runId: string) {
    return this.request<Run>(`/api/runs/${encodeURIComponent(runId)}`);
  }

  startRun(taskId: string, gitRef?: string) {
    return this.request<{ run: Run }>(`/api/tasks/${encodeURIComponent(taskId)}/runs`, {
      method: "POST",
      body: JSON.stringify({
        idempotency_key: crypto.randomUUID(),
        git_ref: gitRef?.trim() || null,
      }),
    });
  }

  cancelRun(runId: string) {
    return this.request<Run>(`/api/runs/${encodeURIComponent(runId)}/cancel`, {
      method: "POST",
    });
  }

  sendMessage(taskId: string, text: string) {
    return this.request<{ status: string; thread_id: string }>(
      `/api/tasks/${encodeURIComponent(taskId)}/messages`,
      { method: "POST", body: JSON.stringify({ text }) },
    );
  }

  listEvents(taskId: string, afterSequence?: number) {
    const query = new URLSearchParams({ limit: "200" });
    if (afterSequence != null) query.set("after_sequence", String(afterSequence));
    return this.request<RunEvent[]>(
      `/api/tasks/${encodeURIComponent(taskId)}/events?${query.toString()}`,
    );
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

  workspaceStatus(runId: string) {
    return this.request<WorkspaceStatus>(
      `/api/runs/${encodeURIComponent(runId)}/workspace/status`,
    );
  }

  commitWorkspace(runId: string, selectedPaths: string[], message: string) {
    return this.request<{ workspace_id: string; commit: string }>(
      `/api/runs/${encodeURIComponent(runId)}/workspace/commit`,
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
