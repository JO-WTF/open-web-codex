import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AppServerEvent, ThreadTokenUsage, WorkspaceInfo } from "./types";
import { CodexMonitorWebClient } from "./services/webClient";
import Layout from "./components/Layout";
import Sidebar from "./components/Sidebar";
import Conversation from "./components/Conversation";
import type { GoalInfo } from "./components/Conversation/GoalBanner";
import { buildWebThreadHistory, commandText } from "./utils/webThreadHistory";
import "./styles/web.css";

/* ─────────── Types ─────────── */

export type LogEntry = {
  id: string;
  level: "event" | "error" | "info" | "user" | "assistant" | "system";
  text: string;
  approvalId?: string;
  approvalRequestId?: number | string;
  kind?: "reasoning" | "tool" | "diff" | "approval" | "command_exec";
  toolType?: string;
  toolTitle?: string;
  toolStatus?: string;
  filePath?: string;
  diffTitle?: string;
  diffLines?: { type: "add" | "del" | "ctx"; text: string }[];
  meta?: string;
  streaming?: boolean;
  cmdExitCode?: number;
  cmdDurationMs?: number;
  cmdCwd?: string;
  cmdOutput?: string;
  cmdActions?: { type: string; path: string }[];
};

type GatewayState = "checking" | "online" | "offline";

type ThreadInfo = {
  id: string;
  label: string;
  updatedAt: number;
  turnCount?: number;
};

/* ─────────── Helpers ─────────── */

function extractThreadId(result: Record<string, unknown> | null | undefined) {
  if (!result) return null;
  const candidates = [result.threadId, result.thread_id, result.id];
  for (const c of candidates) if (typeof c === "string" && c.trim()) return c;
  const thread = result.thread;
  if (thread && typeof thread === "object") {
    const r = thread as Record<string, unknown>;
    if (typeof r.id === "string") return r.id;
    if (typeof r.threadId === "string") return r.threadId;
  }
  // Handle nested start_thread response: {result: {thread: {id: "..."}}}
  const inner = result.result;
  if (inner && typeof inner === "object") {
    return extractThreadId(inner as Record<string, unknown>);
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

const newLogId = () =>
  crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;

/* ─────────── Component ─────────── */

export default function WebApp() {
  console.log('[open-web-codex] build:', '2026-07-12T21:20:00Z');
  const [baseUrl, setBaseUrl] = useState(
    localStorage.getItem("codexMonitorWebBaseUrl") ?? "http://127.0.0.1:4733",
  );
  const [token, setToken] = useState(sessionStorage.getItem("codexMonitorWebToken") ?? "");
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [activeThreadId, setActiveThreadId] = useState<string | null>(null);
  const [threadsByWorkspace, setThreadsByWorkspace] = useState<Record<string, ThreadInfo[]>>({});
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [messages, setMessages] = useState<LogEntry[]>([]);
  const [gatewayState, setGatewayState] = useState<GatewayState>("checking");
  const [gatewayVersion, setGatewayVersion] = useState<string | null>(null);
  const [thinking, setThinking] = useState(false);
  const [tokenUsage, setTokenUsage] = useState<ThreadTokenUsage | null>(null);
  const [threadStatus, setThreadStatus] = useState<string>("idle");
  const [threadSettings, setThreadSettings] = useState<Record<string, unknown> | null>(null);
  const [rateLimits, setRateLimits] = useState<Record<string, unknown> | null>(null);
  const [goal, setGoal] = useState<GoalInfo | null>(null);
  const [mcpServers, setMcpServers] = useState<Record<string, {name: string; status: string; error?: string | null; failureReason?: string | null}>>({});

  const client = useMemo(() => new CodexMonitorWebClient({ baseUrl, token }), [baseUrl, token]);
  const activeWorkspace = workspaces.find((w) => w.id === activeWorkspaceId) ?? null;

  // Streaming accumulators
  const streamingTexts = useRef<Map<string, string>>(new Map());
  const streamingLogIds = useRef<Map<string, string>>(new Map());
  const selectThreadRef = useRef<((threadId: string) => Promise<void>) | null>(null);
  const activeThreadIdRef = useRef<string | null>(activeThreadId);
  activeThreadIdRef.current = activeThreadId;

  const appendLog = useCallback(
    (level: LogEntry["level"], text: string, extra?: Partial<Omit<LogEntry, "id" | "level" | "text">>) => {
      setMessages((prev) => [
        ...prev.slice(-199),
        { id: newLogId(), level, text, ...extra },
      ]);
    },
    [],
  );

  const handleAppEvent = useCallback(
    (event: AppServerEvent): (Partial<Omit<LogEntry, "id" | "level" | "text">> & { level: LogEntry["level"]; text: string }) | null => {
      const message = event.message ?? {};
      const method = typeof message.method === "string" ? message.method : null;
      if (!method) return null;

      const params =
        message.params && typeof message.params === "object"
          ? (message.params as Record<string, unknown>)
          : {};

      const itemId = typeof params.itemId === "string" ? params.itemId : null;
      const delta = typeof params.delta === "string" ? params.delta : "";

      switch (method) {
        case "turn/started":
          setThinking(true);
          return { level: "system", text: "Thinking..." };

        case "turn/completed":
         setThinking(false);
         const completedThreadId = typeof params.threadId === "string"
           ? params.threadId
           : typeof params.thread_id === "string" ? params.thread_id : null;
         if (completedThreadId && completedThreadId === activeThreadIdRef.current) {
           // The completed item event can omit reasoning summaries. Once a turn is
           // durable, refresh from the Runtime's thread projection as the source of truth.
           void selectThreadRef.current?.(completedThreadId);
         }
         return { level: "system", text: "Turn complete" };

        case "thread/started": {
          const thread = params.thread as Record<string, unknown> | undefined;
          const threadName = typeof thread?.name === "string" && thread.name ? thread.name : null;
          const cliVersion = typeof thread?.cliVersion === "string" ? thread.cliVersion : null;
          return {
            level: "info",
            text: `Thread started${threadName ? `: ${threadName}` : ""}${cliVersion ? ` (${cliVersion})` : ""}`,
          };
        }

        case "mcpServer/startupStatus/updated": {
          const serverName = typeof params.name === "string" ? params.name : "";
          const status = typeof params.status === "string" ? params.status : "";
          if (!serverName) return null;
          setMcpServers(prev => ({
            ...prev,
            [serverName]: {
              name: serverName,
              status,
              error: params.error != null ? String(params.error) : null,
              failureReason: typeof params.failureReason === "string" ? params.failureReason : null,
            },
          }));
          if (status === "error") {
            const msg = typeof params.failureReason === "string" && params.failureReason
              ? `: ${params.failureReason}` : "";
            return { level: "error" as const, text: `MCP ${serverName} error${msg}` };
          }
          return null;
        }

        case "item/reasoning/summaryTextDelta":
        case "item/reasoning/textDelta": {
          if (!itemId || !delta) return null;
          const key = `reason_${itemId}`;
          const current = streamingTexts.current.get(key) ?? "";
          const updated = current + delta;
          streamingTexts.current.set(key, updated);

          const existingLogId = streamingLogIds.current.get(key);
          if (existingLogId) {
            setMessages((prev) =>
              prev.map((e) => (e.id === existingLogId ? { ...e, text: updated } : e)),
            );
            return null;
          }
          const id = newLogId();
          streamingLogIds.current.set(key, id);
          setMessages((prev) => [
            ...prev.slice(-199),
            { id, level: "system", text: updated, kind: "reasoning" },
          ]);
          return null;
        }

        case "item/agentMessage/delta": {
          if (!itemId) return null;
          if (!delta) return null;
          if (streamingTexts.current.has(`reason_${itemId}`)) return null;

          const current = streamingTexts.current.get(itemId) ?? "";
          const updated = current + delta;
          streamingTexts.current.set(itemId, updated);

          const existingLogId = streamingLogIds.current.get(itemId);
          if (existingLogId) {
            setMessages((prev) =>
              prev.map((e) => (e.id === existingLogId ? { ...e, text: updated, streaming: true } : e)),
            );
            return null;
          }
          const id = newLogId();
          streamingLogIds.current.set(itemId, id);
          setMessages((prev) => [
            ...prev.slice(-199),
            { id, level: "assistant", text: updated, streaming: true },
          ]);
          return null;
        }

        case "item/completed": {
          const item = params.item as Record<string, unknown> | undefined;
          if (!item) return null;
          const role = typeof item.role === "string" ? item.role : null;
          const kind = typeof item.kind === "string" ? item.kind : null;
          const itemIdFromItem = typeof item?.id === "string" ? item.id : null;

          if (role === "user") return null;

          if (role === "assistant") {
            if (itemId && streamingTexts.current.has(itemId)) {
              const acc = streamingTexts.current.get(itemId)!;
              const eid = streamingLogIds.current.get(itemId);
              if (eid)
                setMessages((prev) =>
                  prev.map((e) => (e.id === eid ? { ...e, text: acc, streaming: false } : e)),
                );
              streamingTexts.current.delete(itemId);
              streamingLogIds.current.delete(itemId);
              return null;
            }
            return {
              level: "assistant",
              text: (typeof item.text === "string" ? item.text : "") || "(no response)",
            };
          }

          if (kind === "tool") {
            const toolType = typeof item.toolType === "string" ? item.toolType : "";
            const title = typeof item.title === "string" ? item.title : "";
            const status = typeof item.status === "string" ? item.status : "";
            const filePath = typeof item.filePath === "string" ? item.filePath : undefined;
            return {
              level: "info" as const,
              text: `${toolType}: ${title}`,
              kind: "tool" as const,
              toolType,
              toolTitle: title,
              toolStatus: status,
              filePath,
            };
          }

          if (kind === "reasoning") {
            const key = itemId ? `reason_${itemId}` : (itemIdFromItem ? `reason_${itemIdFromItem}` : null);
            if (key && streamingTexts.current.has(key)) {
              const acc = streamingTexts.current.get(key)!;
              const eid = streamingLogIds.current.get(key);
              if (eid && acc.trim()) {
                setMessages((prev) =>
                  prev.map((e) => (e.id === eid ? { ...e, text: acc } : e)),
                );
              }
              streamingTexts.current.delete(key);
              streamingLogIds.current.delete(key);
              if (eid && acc.trim()) return null;
            }
            const summary = (Array.isArray(item.summary) && (item.summary as unknown[]).length > 0)
              ? (item.summary as unknown[]).map(s => String(s)).join("\n\n")
              : typeof item.summary === "string" ? item.summary : "";
            const finalText = summary.trim() || (key && streamingTexts.current.has(key) ? (streamingTexts.current.get(key) ?? "").trim() : "");
            return finalText
              ? { level: "system" as const, text: finalText, kind: "reasoning" as const }
              : null;
          }

          if (kind === "diff") {
            const title = typeof item.title === "string" ? item.title : "";
            const diff = typeof item.diff === "string" ? item.diff : "";
            const lines = diff
              .split("\n")
              .filter(Boolean)
              .map((l: string) => {
                if (l.startsWith("+")) return { type: "add" as const, text: l.slice(1) };
                if (l.startsWith("-")) return { type: "del" as const, text: l.slice(1) };
                return { type: "ctx" as const, text: l };
              });
            return {
              level: "info" as const,
              text: title,
              kind: "diff" as const,
              diffTitle: title,
              diffLines: lines.slice(0, 100),
            };
          }


          const itemType2 = typeof item.type === "string" ? item.type : null;

          if (itemType2 === "reasoning") {
            const key = itemId ? `reason_${itemId}` : (itemIdFromItem ? `reason_${itemIdFromItem}` : null);
            if (key && streamingTexts.current.has(key)) {
              const acc = streamingTexts.current.get(key)!;
              const eid = streamingLogIds.current.get(key);
              if (eid && acc.trim()) {
                setMessages((prev) =>
                  prev.map((e) => (e.id === eid ? { ...e, text: acc } : e)),
                );
              }
              streamingTexts.current.delete(key);
              streamingLogIds.current.delete(key);
              if (eid && acc.trim()) return null;
            }
            const summary = (Array.isArray(item.summary) && (item.summary as unknown[]).length > 0)
              ? (item.summary as unknown[]).map(s => String(s)).join("\n\n")
              : typeof item.summary === "string" ? item.summary : "";
            const finalText = summary.trim() || (key && streamingTexts.current.has(key) ? (streamingTexts.current.get(key) ?? "").trim() : "");
            return finalText
              ? { level: "system" as const, text: finalText, kind: "reasoning" as const }
              : null;
          }

          if (itemType2 === "commandExecution") {
            const cmd = commandText(item.command);
            const output = typeof item.aggregatedOutput === "string" ? item.aggregatedOutput : "";
            const exitCode = typeof item.exitCode === "number" ? item.exitCode : undefined;
            const durationMs = typeof item.durationMs === "number" ? item.durationMs : undefined;
            const cwd = typeof item.cwd === "string" ? item.cwd : undefined;
            const cmdActions = Array.isArray(item.commandActions)
              ? (item.commandActions as Record<string,unknown>[]).map(a => ({type: String(a.type ?? ""), path: String(a.path ?? "")}))
              : [];
            return {
              level: "info" as const,
              text: cmd,
              kind: "command_exec" as const,
              cmdOutput: output,
              toolStatus: typeof item.status === "string" ? item.status : undefined,
              cmdExitCode: exitCode,
              cmdDurationMs: durationMs,
              cmdCwd: cwd,
              cmdActions,
            };
          }
          return null;
        }

        case "item/started": {
          const item = params.item as Record<string, unknown> | undefined;
          const itemKind = typeof item?.kind === "string" ? item.kind : null;
          const itemType = typeof item?.type === "string" ? item.type : null;
          if (itemKind === "tool") {
            return {
              level: "info" as const,
              text: "",
              kind: "tool" as const,
              toolType: "",
              toolTitle: "",
              toolStatus: "running",
            };
          }
          if (itemKind === "reasoning" || itemType === "reasoning") return null;
          return null;
        }

        case "turn/error": {
          const msg = typeof params.message === "string" ? params.message : "Unknown error";
          setThinking(false);
          return { level: "error" as const, text: `Turn error: ${msg}` };
        }

        case "thread/status/changed": {
          const status = params.status as Record<string, unknown> | undefined;
          const type = typeof status?.type === "string" ? status.type : "unknown";
          const activeFlags = Array.isArray(status?.activeFlags) ? (status as Record<string, unknown>).activeFlags as string[] : [];
          const flagsStr = activeFlags.length > 0 ? ":" + activeFlags.join(",") : "";
          setThreadStatus(type + flagsStr);
          if (type === "error") {
            return { level: "error" as const, text: "Thread error" };
          }
          return null;
        }

        case "turn/plan/updated":
          return { level: "info" as const, text: "Plan updated" };

        case "item/plan/ready":
          return { level: "info" as const, text: "Plan ready" };

        case "turn/diff/updated":
          return { level: "info" as const, text: "Diff updated" };

        case "thread/tokenUsage/updated": {
          const raw = params.tokenUsage as Record<string, unknown> | undefined;
          if (raw?.total && typeof raw.total === "object") {
            const tu: ThreadTokenUsage = {
              total: raw.total as ThreadTokenUsage["total"],
              last: raw.last as ThreadTokenUsage["last"],
              modelContextWindow:
                typeof raw.modelContextWindow === "number" ? raw.modelContextWindow : null,
            };
            setTokenUsage(tu);
          }
          return null;
        }

        case "thread/settings/updated": {
          const s = params as Record<string, unknown>;
          setThreadSettings(s);
          return null;
        }

        case "item/reasoning/summaryPartAdded": {
          const partItemId = typeof params.itemId === "string" ? params.itemId : null;
          if (partItemId) {
            const key = `reason_${partItemId}`;
            // Keep streaming entry alive — just add a paragraph break between parts
            const current = streamingTexts.current.get(key) ?? "";
            if (current && !current.endsWith("\n\n")) {
              streamingTexts.current.set(key, current + "\n\n");
            }
          }
          return null;
        }

        case "thread/goal/updated": {
          const g = params.goal as Record<string, unknown> | undefined;
          if (g) {
            setGoal({
              objective: typeof g.objective === "string" ? g.objective : "",
              status: typeof g.status === "string" ? g.status : "active",
              tokenBudget: typeof g.tokenBudget === "number" ? g.tokenBudget : null,
              tokensUsed: typeof g.tokensUsed === "number" ? g.tokensUsed : 0,
              timeUsedSeconds: typeof g.timeUsedSeconds === "number" ? g.timeUsedSeconds : 0,
            });
          }
          return null;
        }

        case "thread/goal/cleared": {
          setGoal(null);
          return null;
        }

        case "account/rateLimits/updated": {
          const raw = params.rateLimits as Record<string, unknown> | undefined;
          if (raw) setRateLimits(raw);
          return null;
        }

        case "item/commandExecution/requestApproval": {
          const cmd = commandText(params.command);
          if (!cmd) return null;
          const requestId = message.id;
          return {
            level: "info" as const,
            text: cmd,
            kind: "approval" as const,
            approvalId: typeof params.itemId === "string" ? params.itemId : undefined,
            approvalRequestId:
              typeof requestId === "number" || typeof requestId === "string"
                ? requestId
                : undefined,
          };
        }


        default: {
          const text = summarizeEvent(event);
          return text && text !== "{}" ? { level: "event" as const, text } : null;
        }
      }
    },
    [],
  );

  /* ─── Connection ─── */

  const saveConnection = useCallback(() => {
    localStorage.setItem("codexMonitorWebBaseUrl", baseUrl);
    sessionStorage.setItem("codexMonitorWebToken", token);
  }, [baseUrl, token]);

  const checkGateway = useCallback(async () => {
    setGatewayState("checking");
    try {
      const health = await client.health();
      setGatewayState("online");
      setGatewayVersion(health.version);
      saveConnection();
      return true;
    } catch (error) {
      setGatewayState("offline");
      setGatewayVersion(null);
      appendLog("error", error instanceof Error ? error.message : String(error));
      return false;
    }
  }, [appendLog, client, saveConnection]);

  const refreshWorkspaces = useCallback(async () => {
    setBusy(true);
    try {
      const next = await client.listWorkspaces();
      setWorkspaces(next);
      setActiveWorkspaceId((cur) => cur ?? next[0]?.id ?? null);
    } catch (error) {
      appendLog("error", error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, [appendLog, client]);

  const refreshThreads = useCallback(async (forWorkspaceId?: string) => {
    const wid = forWorkspaceId ?? activeWorkspaceId;
    if (!wid) return;
    try {
      const raw = await client.listThreads(wid);
      const inner = (raw as Record<string, unknown>)?.result;
      const allData =
        (inner as Record<string, unknown>)?.data ?? raw ?? [];
      // Filter to this workspace by cwd
      const ws = workspaces.find(w => w.id === wid);
      const wsPath = ws?.path ?? '';
      const arr = Array.isArray(allData)
        ? allData.filter((t: Record<string, unknown>) =>
            wsPath ? String(t.cwd ?? '').startsWith(wsPath) : true)
        : [];
      if (arr.length > 0 || Array.isArray(arr)) {
        setThreadsByWorkspace(prev => {
          const received = arr.map((t: Record<string, unknown>) => ({
            id: String(t.id ?? ""),
            label: String(t.name ?? t.label ?? "Thread"),
            updatedAt: typeof t.updatedAt === "number" ? t.updatedAt : 0,
            turnCount: typeof t.turnCount === "number" ? t.turnCount : undefined,
          }));
          // Newly started threads are not listed until their first persisted turn.
          // Preserve the optimistic entry until Runtime returns the canonical row.
          const pending = (prev[wid] ?? []).filter(
            (thread) => thread.label === "New thread" && !received.some((next) => next.id === thread.id),
          );
          return { ...prev, [wid]: [...pending, ...received] };
        });
      }
    } catch { /* not fatal */ }
  }, [activeWorkspaceId, client, workspaces]);

  const connectWorkspace = useCallback(async (id: string) => {
    if (!activeWorkspaceId || activeWorkspaceId !== id) {
      setActiveWorkspaceId(id);
    }
    try {
      await client.connectWorkspace(id);
      await refreshWorkspaces();
      // threads will refresh via the activeWorkspaceId effect
    } catch { /* not fatal */ }
  }, [client, refreshWorkspaces]);

 useEffect(() => {
   void checkGateway();
    void refreshWorkspaces();
   const unsub = client.subscribeAppServerEvents(
      (event) => {
        // Accept events for any workspace; caller filters
        const wsId = activeWorkspaceId;
        if (wsId && event.workspace_id !== wsId) return;
        const tid = extractThreadIdFromEvent(event);
        if (tid) setActiveThreadId(tid);
        const entry = handleAppEvent(event);
        if (entry) {
          const { level, text, ...extra } = entry;
          appendLog(level, text, extra);
        }
      },
      { onOpen: () => setGatewayState("online"), onError: () => setGatewayState("offline") },
    );
    return unsub;
  }, [appendLog, checkGateway, client]);

  // Auto-refresh threads when workspace changes
  useEffect(() => {
    void refreshThreads();
  }, [activeWorkspaceId, refreshThreads]);

  /* ─── Workspace actions ─── */


  const createWorkspace = useCallback(
    async (name: string) => {
      setBusy(true);
      try {
        await client.createWorkspace(name);
        await refreshWorkspaces();
      } catch (error) {
        appendLog("error", error instanceof Error ? error.message : String(error));
      } finally {
        setBusy(false);
      }
    },
    [appendLog, client, refreshWorkspaces],
  );


 const startThread = useCallback(async (workspaceId?: string) => {
   const wid = workspaceId ?? activeWorkspaceId;
   if (!wid) return;
   setBusy(true);
   try {
     await connectWorkspace(wid);
     setActiveWorkspaceId(wid);
     const result = await client.startThread(wid);
     // Handle Codex CLI JSON-RPC error embedded in result
     if (result && typeof result === "object" && "error" in result) {
       const err = (result as Record<string,unknown>).error as Record<string,unknown> | undefined;
       const msg = typeof err?.message === "string" ? err.message : JSON.stringify(err);
       throw new Error(msg);
     }
     const tid = extractThreadId(result);
     if (tid) {
       // A new thread must begin with a clean transcript, even before its first
       // user message makes it into `thread/list`.
       setActiveThreadId(tid);
       setMessages([]);
       setTokenUsage(null);
       setGoal(null);
       setThinking(false);
       setThreadStatus("idle");
       setThreadsByWorkspace((previous) => {
         const existing = previous[wid] ?? [];
         if (existing.some((thread) => thread.id === tid)) {
           return previous;
         }
         return {
           ...previous,
           [wid]: [{ id: tid, label: "New thread", updatedAt: Date.now() }, ...existing],
         };
       });
     }
     await refreshThreads(wid);
     appendLog("info", `Started thread${tid ? ` ${tid}` : ""}.`);
   } catch (error) {
     appendLog("error", error instanceof Error ? error.message : String(error));
   } finally {
     setBusy(false);
   }
  }, [activeWorkspaceId, appendLog, client, connectWorkspace, refreshThreads]);

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

  const resolveApproval = useCallback(async (
    workspaceId: string,
    requestId: number | string,
    decision: "accept" | "decline",
  ) => {
    try {
      await client.respondToServerRequest(workspaceId, requestId, { decision });
      setMessages((previous) => previous.filter(
        (entry) => entry.approvalRequestId !== requestId,
      ));
    } catch (error) {
      appendLog("error", error instanceof Error ? error.message : String(error));
    }
  }, [appendLog, client]);

  /* ─── Thread management ─── */

  const selectThread = useCallback(async (id: string) => {
    setActiveThreadId(id);
    setMessages([]);
    setTokenUsage(null);
    setGoal(null);
    const wid = activeWorkspaceId;
    if (!wid) return;
    try {
      let resumed: Record<string, unknown> | null = null;
      // Resuming an already-active thread may be rejected; history remains readable.
      try {
        resumed = await client.resumeThread(wid, id);
      } catch {
        // Best effort: `thread/read` below is the source for the browser projection.
      }
      // `thread/resume` is the authoritative loaded-thread projection. In
      // particular it preserves the reasoning summaries for interrupted turns;
      // `thread/read` may legitimately return an empty turns projection.
      const raw = resumed ?? await client.readThread(wid, id);
      const inner = (raw as Record<string, unknown>)?.result;
      let obj = (inner ?? raw) as Record<string, unknown>;
      let thread = obj.thread as Record<string, unknown> | undefined;
      let turns = Array.isArray(thread?.turns) ? thread.turns as Record<string, unknown>[] : (Array.isArray(obj.turns) ? obj.turns as Record<string, unknown>[] : []);
      const loaded = buildWebThreadHistory({ turns }, newLogId);
      if (loaded.length > 0) setMessages(loaded);
    } catch { /* history load is best-effort */ }
  }, [activeWorkspaceId, client]);

  selectThreadRef.current = selectThread;

  /* ─── Render ─── */

  return (
    <Layout
      sidebar={
        <Sidebar
          gatewayState={gatewayState}
          gatewayVersion={gatewayVersion}
          workspaces={workspaces}
          activeWorkspaceId={activeWorkspaceId}
          onSelectWorkspace={setActiveWorkspaceId}
          threadsByWorkspace={threadsByWorkspace}
          activeThreadId={activeThreadId}
          onCreateWorkspace={createWorkspace}

          onSelectThread={selectThread}
          onNewThread={startThread}
          baseUrl={baseUrl}
          token={token}
          onBaseUrlChange={setBaseUrl}
          onTokenChange={setToken}
          onCheckGateway={checkGateway}
          onLoadWorkspaces={refreshWorkspaces}
          busy={busy}
          mcpServers={mcpServers}
          rateLimits={rateLimits}

          onConnectWorkspace={connectWorkspace}
        />
      }
    >
      <Conversation
        goal={goal}
        workspaceName={activeWorkspace?.name ?? null}
        threadTitle={activeThreadId ? activeThreadId.slice(0, 12) + "…" : null}
        conversationId={activeThreadId}
          tokenUsage={tokenUsage}
          threadStatus={threadStatus}
          threadSettings={threadSettings}

        messages={messages}
        workspaceId={activeWorkspaceId ?? undefined}
        draft={draft}
        onDraftChange={setDraft}
        onSend={sendMessage}
        busy={busy}
        sendDisabled={!activeWorkspaceId || !activeThreadId}
        thinking={thinking}
        onResolveApproval={resolveApproval}
      />
    </Layout>
  );
}
