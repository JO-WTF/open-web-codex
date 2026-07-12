import { useCallback, useEffect, useMemo, useState } from "react";
import type { AppServerEvent, WorkspaceInfo } from "./types";
import { CodexMonitorWebClient } from "./services/webClient";
import "./styles/web.css";

type LogEntry = {
  id: string;
  level: "event" | "error" | "info" | "user";
  text: string;
};

type GatewayState = "checking" | "online" | "offline";

function extractThreadId(result: Record<string, unknown> | null | undefined) {
  if (!result) return null;
  const candidates = [result.threadId, result.thread_id, result.id];
  for (const candidate of candidates) {
    if (typeof candidate === "string" && candidate.trim()) {
      return candidate;
    }
  }
  const thread = result.thread;
  if (thread && typeof thread === "object") {
    const record = thread as Record<string, unknown>;
    if (typeof record.id === "string") return record.id;
    if (typeof record.threadId === "string") return record.threadId;
  }
  return null;
}

function summarizeEvent(event: AppServerEvent) {
  const message = event.message ?? {};
  const method = typeof message.method === "string" ? message.method : "app-server-event";
  const params =
    message.params && typeof message.params === "object"
      ? (message.params as Record<string, unknown>)
      : {};
  const threadId =
    typeof params.threadId === "string"
      ? params.threadId
      : typeof params.thread_id === "string"
        ? params.thread_id
        : null;
  const item = params.item && typeof params.item === "object" ? params.item : null;
  const text = item ? JSON.stringify(item).slice(0, 240) : JSON.stringify(params).slice(0, 240);
  return `${method}${threadId ? ` · ${threadId}` : ""}${text && text !== "{}" ? ` · ${text}` : ""}`;
}

function extractThreadIdFromEvent(event: AppServerEvent) {
  const message = event.message ?? {};
  if (message.method !== "thread/started") return null;
  const params =
    message.params && typeof message.params === "object"
      ? (message.params as Record<string, unknown>)
      : null;
  return extractThreadId(params);
}

export default function WebApp() {
  const [baseUrl, setBaseUrl] = useState(
    localStorage.getItem("codexMonitorWebBaseUrl") ?? "http://127.0.0.1:4733",
  );
  const [token, setToken] = useState(sessionStorage.getItem("codexMonitorWebToken") ?? "");
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [newWorkspacePath, setNewWorkspacePath] = useState("");
  const [busy, setBusy] = useState(false);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [gatewayState, setGatewayState] = useState<GatewayState>("checking");
  const [gatewayVersion, setGatewayVersion] = useState<string | null>(null);

  const client = useMemo(() => new CodexMonitorWebClient({ baseUrl, token }), [baseUrl, token]);
  const activeWorkspace = workspaces.find((workspace) => workspace.id === activeWorkspaceId) ?? null;

  const appendLog = useCallback((level: LogEntry["level"], text: string) => {
    setLogs((current) => [
      ...current.slice(-199),
      { id: `${Date.now()}-${Math.random().toString(36).slice(2)}`, level, text },
    ]);
  }, []);

  const saveConnection = useCallback(() => {
    localStorage.setItem("codexMonitorWebBaseUrl", baseUrl);
    sessionStorage.setItem("codexMonitorWebToken", token);
    appendLog("info", "Saved web gateway connection settings.");
  }, [appendLog, baseUrl, token]);

  const checkGateway = useCallback(async () => {
    setGatewayState("checking");
    try {
      const health = await client.health();
      setGatewayState("online");
      setGatewayVersion(health.version);
      return true;
    } catch (error) {
      setGatewayState("offline");
      setGatewayVersion(null);
      appendLog("error", error instanceof Error ? error.message : String(error));
      return false;
    }
  }, [appendLog, client]);

  const refreshWorkspaces = useCallback(async () => {
    setBusy(true);
    try {
      const next = await client.listWorkspaces();
      setWorkspaces(next);
      setActiveWorkspaceId((current) => current ?? next[0]?.id ?? null);
      appendLog("info", `Loaded ${next.length} workspace(s).`);
    } catch (error) {
      appendLog("error", error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [appendLog, client]);

  useEffect(() => {
    void checkGateway();
    const unsubscribe = client.subscribeAppServerEvents(
      (event) => {
        if (activeWorkspaceId && event.workspace_id !== activeWorkspaceId) {
          return;
        }
        const startedThreadId = extractThreadIdFromEvent(event);
        if (startedThreadId) {
          setActiveThreadId(startedThreadId);
        }
        appendLog("event", summarizeEvent(event));
      },
      {
        onOpen: () => setGatewayState("online"),
        onError: () => setGatewayState("offline"),
      },
    );
    return unsubscribe;
  }, [activeWorkspaceId, appendLog, checkGateway, client]);

  const connectWorkspace = useCallback(async () => {
    if (!activeWorkspaceId) return;
    setBusy(true);
    try {
      await client.connectWorkspace(activeWorkspaceId);
      appendLog("info", `Connected workspace ${activeWorkspaceId}.`);
      await refreshWorkspaces();
    } catch (error) {
      appendLog("error", error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [activeWorkspaceId, appendLog, client, refreshWorkspaces]);

  const startThread = useCallback(async () => {
    if (!activeWorkspaceId) return;
    setBusy(true);
    try {
      const result = await client.startThread(activeWorkspaceId);
      const threadId = extractThreadId(result);
      if (threadId) {
        setActiveThreadId(threadId);
      }
      appendLog("info", `Started thread${threadId ? ` ${threadId}` : ""}.`);
    } catch (error) {
      appendLog("error", error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [activeWorkspaceId, appendLog, client]);

  const sendMessage = useCallback(async () => {
    const text = draft.trim();
    if (!activeWorkspaceId || !activeThreadId || !text) return;
    setDraft("");
    appendLog("user", text);
    setBusy(true);
    try {
      await client.sendUserMessage(activeWorkspaceId, activeThreadId, text);
    } catch (error) {
      appendLog("error", error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [activeThreadId, activeWorkspaceId, appendLog, client, draft]);

  return (
    <main className="web-app-shell">
      <aside className="web-sidebar">
        <div className="web-brand">
          <div>
            <span className="web-kicker">Local MVP</span>
            <h1>open-web-codex</h1>
          </div>
          <span className={`web-status web-status-${gatewayState}`}>
            {gatewayState === "checking" ? "Checking" : gatewayState}
          </span>
        </div>
        <p className="web-intro">
          Run Codex in a local workspace from your browser. This MVP binds the gateway to this
          machine only.
        </p>
        {gatewayVersion ? <p className="web-version">Gateway v{gatewayVersion}</p> : null}
        <label>
          Gateway URL
          <input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} />
        </label>
        <label>
          Token
          <input value={token} onChange={(event) => setToken(event.target.value)} type="password" />
        </label>
        <div className="web-actions">
          <button
            onClick={() => {
              saveConnection();
              void checkGateway();
            }}
          >
            Save &amp; check
          </button>
          <button onClick={refreshWorkspaces} disabled={busy}>Load workspaces</button>
        </div>
        <label>
          Workspace
          <select
            value={activeWorkspaceId ?? ""}
            onChange={(event) => setActiveWorkspaceId(event.target.value || null)}
          >
            <option value="">Select a workspace</option>
            {workspaces.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>
                {workspace.name} {workspace.connected ? "●" : "○"}
              </option>
            ))}
          </select>
        </label>
        {activeWorkspace ? <p className="web-path">{activeWorkspace.path}</p> : null}
        <div className="web-actions">
          <button onClick={connectWorkspace} disabled={!activeWorkspaceId || busy}>Connect</button>
          <button onClick={startThread} disabled={!activeWorkspaceId || busy}>New thread</button>
        </div>
        <label>
          Add workspace
          <input
            value={newWorkspacePath}
            onChange={(event) => setNewWorkspacePath(event.target.value)}
            placeholder="/path/to/workspace"
          />
        </label>
        <div className="web-actions">
          <button
            onClick={async () => {
              if (newWorkspacePath.trim()) {
                setBusy(true);
                try {
                  await client.addWorkspace(newWorkspacePath.trim());
                  appendLog("info", "Added workspace.");
                  setNewWorkspacePath("");
                  await refreshWorkspaces();
                } catch (error) {
                  appendLog("error", error instanceof Error ? error.message : String(error));
                } finally {
                  setBusy(false);
                }
              }
            }}
            disabled={!newWorkspacePath.trim() || busy}
          >
            Add
          </button>
        </div>
        <label>
          Thread ID
          <input
            value={activeThreadId ?? ""}
            onChange={(event) => setActiveThreadId(event.target.value || null)}
            placeholder="Start a thread or paste an existing ID"
          />
        </label>
      </aside>
      <section className="web-chat">
        <div className="web-log">
          {logs.length === 0 ? (
            <div className="web-empty">
              <div>
                <span className="web-kicker">Ready when you are</span>
                <h2>Start a real Codex task from the browser</h2>
                <ol>
                  <li>Load or add a workspace.</li>
                  <li>Connect it and start a thread.</li>
                  <li>Describe the coding task below.</li>
                </ol>
              </div>
            </div>
          ) : (
            logs.map((entry) => (
              <article key={entry.id} className={`web-log-entry web-log-${entry.level}`}>
                <strong>{entry.level}</strong>
                <pre>{entry.text}</pre>
              </article>
            ))
          )}
        </div>
        <form
          className="web-composer"
          onSubmit={(event) => {
            event.preventDefault();
            void sendMessage();
          }}
        >
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="Ask Codex to perform a task in the selected workspace…"
          />
          <button disabled={busy || !activeWorkspaceId || !activeThreadId || !draft.trim()}>
            {busy ? "Working…" : "Send"}
          </button>
        </form>
      </section>
    </main>
  );
}
