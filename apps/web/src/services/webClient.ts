import type {
  AppServerEvent,
  GitFileStatus,
  ProjectedRunEvent,
  WorkspaceInfo,
} from "../types";

type RpcResponse<T> = { result?: T; error?: { message?: string } | string };

type WebClientOptions = {
  baseUrl?: string;
  token?: string;
};

export type GatewayHealth = {
  ok: boolean;
  name: string;
  version: string;
};

type EventSubscriptionStatus = {
  onOpen?: () => void;
  onError?: () => void;
};

export type ProviderModelSummary = {
  modelId: string;
  modelName?: string | null;
  maxTokenLen?: number | null;
  maxOutputTokens?: number | null;
  showInPicker: boolean;
  contextWindow?: number | null;
};

export type ProviderSummary = {
  id: string;
  name: string;
  baseUrl?: string | null;
  envKey?: string | null;
  wireApi: string;
  kind: "builtIn" | "local" | "custom";
  isCurrent: boolean;
  modelCount: number;
  canEdit: boolean;
  canDelete: boolean;
  canFetchModels: boolean;
  models: ProviderModelSummary[];
};

export type ProviderCatalog = {
  data: ProviderSummary[];
  currentProviderId: string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function unwrapRpcResult(value: unknown): Record<string, unknown> {
  let current = value;
  while (isRecord(current) && isRecord(current.result)) {
    current = current.result;
  }
  return isRecord(current) ? current : {};
}

function defaultBaseUrl() {
  return import.meta.env.VITE_CODEX_MONITOR_WEB_API ?? "http://127.0.0.1:4733";
}

export class CodexMonitorWebClient {
  private baseUrl: string;
  private token: string;

  constructor(options: WebClientOptions = {}) {
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
    let payload: RpcResponse<T> | T | null = null;
    if (body) {
      try {
        payload = JSON.parse(body) as RpcResponse<T> | T;
      } catch {
        throw new Error(`Gateway returned invalid JSON (HTTP ${response.status}).`);
      }
    }
    if (!response.ok) {
      const rpcPayload = payload as RpcResponse<T> | null;
      const error = rpcPayload?.error;
      const payloadRecord: Record<string, unknown> | null = isRecord(payload) ? payload : null;
      const platformMessage = typeof payloadRecord?.message === "string"
        ? payloadRecord.message
        : undefined;
      const message = typeof error === "string" ? error : error?.message ?? platformMessage;
      throw new Error(message ?? `Gateway request failed (HTTP ${response.status}).`);
    }
    return payload as T;
  }

  health() {
    return this.fetchJson<GatewayHealth>("/api/health");
  }

  listTaskEvents(taskId: string, afterSequence?: number, limit = 200) {
    const query = new URLSearchParams({ limit: String(limit) });
    if (afterSequence != null) {
      query.set("after_sequence", String(afterSequence));
    }
    return this.fetchJson<ProjectedRunEvent[]>(
      `/api/tasks/${encodeURIComponent(taskId)}/events?${query.toString()}`,
    );
  }

  async rpc<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    const payload = await this.fetchJson<RpcResponse<T>>("/api/rpc", {
      method: "POST",
      body: JSON.stringify({ method, params, clientVersion: "web" }),
    });
    if (payload.error) {
      const error = payload.error;
      const message = typeof error === "string" ? error : error?.message ?? "Gateway RPC failed.";
      throw new Error(message);
    }
    return payload.result as T;
  }

  listWorkspaces() {
    return this.rpc<WorkspaceInfo[]>("list_workspaces");
  }

  addWorkspace(path: string) {
    return this.rpc<Record<string, unknown>>("add_workspace", { path });
  }

  createWorkspace(name: string, parent_dir?: string) {
    return this.rpc<Record<string, unknown>>("create_workspace", { name, parent_dir });
  }

  removeWorkspace(id: string) {
    return this.rpc<void>("remove_workspace", { id });
  }

  connectWorkspace(workspaceId: string) {
    return this.rpc<void>("connect_workspace", { id: workspaceId });
  }

  startThread(workspaceId: string) {
    return this.rpc<Record<string, unknown>>("start_thread", { workspaceId });
  }

  listModelProviders(_workspaceId: string) {
    return this.fetchJson<ProviderCatalog>("/api/providers");
  }

  writeModelProvider(_workspaceId: string, input: Record<string, unknown>) {
    const action = typeof input.action === "string" ? input.action : "";
    const id = typeof input.id === "string" ? input.id : "";
    const encodedId = encodeURIComponent(id);
    if (action === "select") {
      return this.fetchJson<ProviderCatalog>(`/api/providers/${encodedId}/select`, {
        method: "POST",
      });
    }
    if (action === "delete") {
      return this.fetchJson<ProviderCatalog>(`/api/providers/${encodedId}`, {
        method: "DELETE",
      });
    }
    if (action === "fetch") {
      return this.fetchJson<ProviderCatalog>(`/api/providers/${encodedId}/models/refresh`, {
        method: "POST",
      });
    }
    if (action === "context") {
      const modelId = typeof input.modelId === "string" ? input.modelId : "";
      return this.fetchJson<ProviderCatalog>(
        `/api/providers/${encodedId}/models/${encodeURIComponent(modelId)}`,
        {
          method: "PATCH",
          body: JSON.stringify({ contextWindow: input.contextWindow }),
        },
      );
    }
    if (action === "upsert") {
      const credentialMode = typeof input.credentialMode === "string"
        ? input.credentialMode
        : "none";
      const credentials = credentialMode === "environment"
        ? { mode: "environment", envKey: input.envKey }
        : credentialMode === "direct"
          ? { mode: "direct", apiKey: input.apiKey }
          : credentialMode === "preserve"
            ? { mode: "preserve" }
            : { mode: "none" };
      return this.fetchJson<ProviderCatalog>(`/api/providers/${encodedId}`, {
        method: "PUT",
        body: JSON.stringify({
          name: input.name,
          baseUrl: input.baseUrl,
          wireApi: input.wireApi,
          credentials,
          select: input.select === true,
        }),
      });
    }
    return Promise.reject(new Error(`Unsupported Provider action '${action}'.`));
  }

  listModels(workspaceId: string) {
    return this.rpc<Record<string, unknown>>("model_list", { workspaceId });
  }

  listMcpServerStatus(workspaceId: string) {
    return this.rpc<Record<string, unknown>>("list_mcp_server_status", {
      workspaceId,
      limit: 100,
    });
  }

  getAccountRateLimits(workspaceId: string) {
    return this.rpc<Record<string, unknown>>("account_rate_limits", { workspaceId });
  }

  resumeThread(workspaceId: string, threadId: string) {
    return this.rpc<Record<string, unknown>>("resume_thread", { workspaceId, threadId });
  }

  readThread(workspaceId: string, threadId: string) {
    return this.rpc<Record<string, unknown>>("read_thread", { workspaceId, threadId });
  }

  async listThreadTurns(workspaceId: string, threadId: string) {
    const turns: Record<string, unknown>[] = [];
    let cursor: string | null = null;
    do {
      const raw = await this.rpc<Record<string, unknown>>("list_thread_turns", {
        workspaceId,
        threadId,
        ...(cursor ? { cursor } : {}),
      });
      const page = unwrapRpcResult(raw);
      if (Array.isArray(page.data)) {
        turns.push(...page.data.filter(isRecord));
      }
      cursor = typeof page.nextCursor === "string" && page.nextCursor
        ? page.nextCursor
        : null;
    } while (cursor);
    return turns.reverse();
  }

  async listThreads(workspaceId: string) {
    const threads: Record<string, unknown>[] = [];
    let cursor: string | null = null;
    do {
      const raw = await this.rpc<Record<string, unknown>>("list_threads", {
        workspaceId,
        limit: 100,
        sortKey: "updated_at",
        ...(cursor ? { cursor } : {}),
      });
      const page = unwrapRpcResult(raw);
      if (Array.isArray(page.data)) {
        threads.push(...page.data.filter(isRecord));
      }
      cursor = typeof page.nextCursor === "string" && page.nextCursor
        ? page.nextCursor
        : null;
    } while (cursor);
    return { data: threads };
  }

  archiveThread(workspaceId: string, threadId: string) {
    return this.rpc<Record<string, unknown>>("archive_thread", { workspaceId, threadId });
  }

  listWorkspaceFiles(workspaceId: string) {
    return this.rpc<string[]>("list_workspace_files", { workspaceId });
  }

  readWorkspaceFile(workspaceId: string, path: string) {
    return this.rpc<{ content: string; truncated: boolean }>("read_workspace_file", { workspaceId, path });
  }

  getGitStatus(workspaceId: string) {
    return this.rpc<{ files: GitFileStatus[] }>("get_git_status", { workspaceId });
  }

  sendUserMessage(workspaceId: string, threadId: string, text: string, model?: string | null) {
    return this.rpc<Record<string, unknown>>("send_user_message", {
      workspaceId,
      threadId,
      text,
      accessMode: "current",
      ...(model ? { model } : {}),
    });
  }

  interruptTurn(workspaceId: string, threadId: string, turnId: string) {
    return this.rpc<Record<string, unknown>>("turn_interrupt", {
      workspaceId,
      threadId,
      turnId,
    });
  }

  steerTurn(workspaceId: string, threadId: string, turnId: string, text: string) {
    return this.rpc<Record<string, unknown>>("turn_steer", {
      workspaceId,
      threadId,
      turnId,
      text,
    });
  }

  subscribeAppServerEvents(
    onEvent: (event: AppServerEvent) => void,
    status: EventSubscriptionStatus = {},
  ) {
    const url = new URL(`${this.baseUrl}/api/events`);
    if (this.token) {
      url.searchParams.set("token", this.token);
    }
    const source = new EventSource(url.toString());
    source.onopen = () => status.onOpen?.();
    source.onerror = () => status.onError?.();
    source.onmessage = (message) => {
      try {
        const payload = JSON.parse(message.data) as {
          method?: string;
          params?: AppServerEvent;
        };
        if (payload.method === "app-server-event" && payload.params) {
          onEvent(payload.params);
        }
      } catch (error) {
        console.warn("Failed to parse CodexMonitor web event", error);
      }
    };
    return () => source.close();
  }

  async resolveApproval(
    workspaceId: string,
    threadId: string,
    decision: string,
  ): Promise<unknown> {
    return this.rpc("resolve_approval", { workspaceId, threadId, decision });
  }

  respondToServerRequest(
    workspaceId: string,
    requestId: number | string,
    result: Record<string, unknown>,
  ): Promise<unknown> {
    return this.rpc("respond_to_server_request", { workspaceId, requestId, result });
  }

}
