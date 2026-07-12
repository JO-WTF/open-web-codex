import type { AppServerEvent, WorkspaceInfo } from "../types";

type RpcResponse<T> = { result?: T; error?: { message?: string } | string };

type WebClientOptions = {
  baseUrl?: string;
  token?: string;
};

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

  async rpc<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
    const response = await fetch(`${this.baseUrl}/api/rpc`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        ...(this.token ? { authorization: `Bearer ${this.token}` } : {}),
      },
      body: JSON.stringify({ method, params, clientVersion: "web" }),
    });
    const payload = (await response.json()) as RpcResponse<T>;
    if (!response.ok || payload.error) {
      const error = payload.error;
      const message =
        typeof error === "string" ? error : error?.message ?? response.statusText;
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

  connectWorkspace(workspaceId: string) {
    return this.rpc<void>("connect_workspace", { id: workspaceId });
  }

  startThread(workspaceId: string) {
    return this.rpc<Record<string, unknown>>("start_thread", { workspaceId });
  }

  listThreads(workspaceId: string) {
    return this.rpc<Record<string, unknown>>("list_threads", { workspaceId, limit: 50 });
  }

  sendUserMessage(workspaceId: string, threadId: string, text: string) {
    return this.rpc<Record<string, unknown>>("send_user_message", {
      workspaceId,
      threadId,
      text,
      accessMode: "current",
    });
  }

  subscribeAppServerEvents(onEvent: (event: AppServerEvent) => void) {
    const url = new URL(`${this.baseUrl}/api/events`);
    if (this.token) {
      url.searchParams.set("token", this.token);
    }
    const source = new EventSource(url.toString());
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
}
