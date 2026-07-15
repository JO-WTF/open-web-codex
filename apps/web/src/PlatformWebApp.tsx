import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Layout from "./components/Layout";
import MessageList from "./components/Conversation/MessageList";
import FileManager from "./components/FileManager";
import type { MessageEntry } from "./components/Conversation/MessageList";
import { PlatformClient } from "./services/platformClient";
import type { PlatformProject, PlatformRun, PlatformRunEvent, PlatformTask } from "./services/platformTypes";
import {
  latestTurnId,
  maxEventSequence,
  projectedEventsToLogEntries,
} from "./utils/projectedRunEventsToLogEntries";
import "./styles/web.css";
import "./styles/web-refactor.css";

type GatewayState = "checking" | "online" | "offline";

const newId = () =>
  crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;

export default function PlatformWebApp() {
  const [baseUrl, setBaseUrl] = useState(
    localStorage.getItem("open-web-codex:api-base") ?? "http://127.0.0.1:4800",
  );
  const [token, setToken] = useState(sessionStorage.getItem("open-web-codex:session") ?? "");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [authError, setAuthError] = useState<string | null>(null);
  const [gatewayState, setGatewayState] = useState<GatewayState>("checking");
  const [gatewayVersion, setGatewayVersion] = useState<string | null>(null);
  const [projects, setProjects] = useState<PlatformProject[]>([]);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [tasks, setTasks] = useState<PlatformTask[]>([]);
  const [activeTaskId, setActiveTaskId] = useState<string | null>(null);
  const [activeRun, setActiveRun] = useState<PlatformRun | null>(null);
  const [messages, setMessages] = useState<MessageEntry[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [thinking, setThinking] = useState(false);
  const [activeTurnId, setActiveTurnId] = useState<string | null>(null);
  const [filePanelOpen, setFilePanelOpen] = useState(false);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectGitUrl, setNewProjectGitUrl] = useState("");
  const [newTaskTitle, setNewTaskTitle] = useState("");
  const eventSequenceRef = useRef(0);
  const eventsRef = useRef<PlatformRunEvent[]>([]);

  const client = useMemo(() => new PlatformClient({ baseUrl, token }), [baseUrl, token]);

  const checkGateway = useCallback(async () => {
    setGatewayState("checking");
    try {
      const health = await client.health();
      setGatewayState(health.ok ? "online" : "offline");
      setGatewayVersion(health.version ?? null);
    } catch {
      setGatewayState("offline");
      setGatewayVersion(null);
    }
  }, [client]);

  useEffect(() => {
    void checkGateway();
  }, [checkGateway]);

  useEffect(() => {
    localStorage.setItem("open-web-codex:api-base", baseUrl);
  }, [baseUrl]);

  useEffect(() => {
    sessionStorage.setItem("open-web-codex:session", token);
  }, [token]);

  const loadProjects = useCallback(async () => {
    const next = await client.listProjects();
    setProjects(next);
    if (!activeProjectId && next.length > 0) {
      setActiveProjectId(next[0].id);
    }
  }, [activeProjectId, client]);

  const loadTasks = useCallback(async (projectId: string) => {
    const next = await client.listTasks(projectId);
    setTasks(next);
    if (!activeTaskId && next.length > 0) {
      setActiveTaskId(next[0].id);
    }
  }, [activeTaskId, client]);

  const refreshRunState = useCallback(async (taskId: string) => {
    const { run } = await client.getActiveRun(taskId);
    setActiveRun(run);
    return run;
  }, [client]);

  const syncTaskEvents = useCallback(async (taskId: string, run: PlatformRun | null) => {
    const events = await client.listTaskEvents(taskId, eventSequenceRef.current || undefined);
    if (events.length > 0) {
      eventsRef.current = [...eventsRef.current, ...events];
      eventSequenceRef.current = maxEventSequence(eventsRef.current);
      setActiveTurnId(latestTurnId(eventsRef.current));
    }
    const approvals = run ? await client.listApprovals(run.id) : [];
    setMessages(projectedEventsToLogEntries(eventsRef.current, approvals));
    const running = run?.status === "running" || run?.status === "provisioning" || run?.status === "queued";
    setThinking(running);
  }, [client]);

  useEffect(() => {
    if (!token) return;
    void loadProjects().catch((error) => setAuthError(String(error)));
  }, [token, loadProjects]);

  useEffect(() => {
    if (!activeProjectId || !token) return;
    void loadTasks(activeProjectId).catch((error) => setAuthError(String(error)));
  }, [activeProjectId, token, loadTasks]);

  useEffect(() => {
    if (!activeTaskId || !token) return;
    eventsRef.current = [];
    eventSequenceRef.current = 0;
    setMessages([]);
    void (async () => {
      const run = await refreshRunState(activeTaskId);
      await syncTaskEvents(activeTaskId, run);
    })().catch((error) => setAuthError(String(error)));
  }, [activeTaskId, token, refreshRunState, syncTaskEvents]);

  useEffect(() => {
    if (!activeTaskId || !token) return undefined;
    const timer = window.setInterval(() => {
      void (async () => {
        const run = await refreshRunState(activeTaskId);
        await syncTaskEvents(activeTaskId, run);
      })().catch(() => undefined);
    }, 1200);
    return () => window.clearInterval(timer);
  }, [activeTaskId, token, refreshRunState, syncTaskEvents]);

  const handleLogin = async () => {
    setAuthError(null);
    setBusy(true);
    try {
      const response = await client.login(email.trim(), password);
      setToken(response.session_token);
      await checkGateway();
      await loadProjects();
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const handleBootstrap = async () => {
    setAuthError(null);
    setBusy(true);
    try {
      const response = await client.bootstrap("Admin", email.trim() || "admin@example.com", password || "changeme");
      setToken(response.session_token);
      await checkGateway();
      await loadProjects();
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const handleCreateProject = async () => {
    if (!newProjectName.trim() || !newProjectGitUrl.trim()) return;
    setBusy(true);
    try {
      const project = await client.createProject({
        name: newProjectName.trim(),
        git_url: newProjectGitUrl.trim(),
      });
      setNewProjectName("");
      setNewProjectGitUrl("");
      setProjects((current) => [project, ...current]);
      setActiveProjectId(project.id);
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteProject = async (projectId: string) => {
    setBusy(true);
    try {
      await client.deleteProject(projectId);
      setProjects((current) => current.filter((project) => project.id !== projectId));
      if (activeProjectId === projectId) {
        setActiveProjectId(null);
        setActiveTaskId(null);
      }
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const handleCreateTask = async () => {
    if (!activeProjectId || !newTaskTitle.trim()) return;
    setBusy(true);
    try {
      const task = await client.createTask(activeProjectId, newTaskTitle.trim());
      setNewTaskTitle("");
      setTasks((current) => [task, ...current]);
      setActiveTaskId(task.id);
      const run = await client.startTaskRun(task.id, newId());
      setActiveRun(run.run);
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const ensureActiveRun = async (taskId: string) => {
    let run = activeRun;
    if (!run || run.task_id !== taskId || ["completed", "cancelled", "failed"].includes(run.status)) {
      const started = await client.startTaskRun(taskId, newId());
      run = started.run;
      setActiveRun(run);
    }
    return run;
  };

  const handleSend = async () => {
    const text = draft.trim();
    if (!activeTaskId || !text) return;
    setBusy(true);
    try {
      await ensureActiveRun(activeTaskId);
      await client.sendMessage(activeTaskId, text);
      setDraft("");
      setMessages((current) => [
        ...current,
        { id: newId(), level: "user", text },
      ]);
      const run = await refreshRunState(activeTaskId);
      await syncTaskEvents(activeTaskId, run);
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const handleInterrupt = async () => {
    if (!activeRun?.id || !activeTurnId) return;
    setBusy(true);
    try {
      await client.interruptRun(activeRun.id, activeTurnId);
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const handleDecideApproval = async (approvalId: string, decision: "approved" | "rejected") => {
    setBusy(true);
    try {
      await client.decideApproval(approvalId, decision);
      if (activeTaskId) {
        const run = await refreshRunState(activeTaskId);
        await syncTaskEvents(activeTaskId, run);
      }
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  if (!token) {
    return (
      <div className="web-auth-shell">
        <div className="web-auth-card">
          <h1>open-web-codex</h1>
          <p>Sign in to the platform API.</p>
          <label>
            API base URL
            <input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} />
          </label>
          <label>
            Email
            <input value={email} onChange={(event) => setEmail(event.target.value)} autoComplete="username" />
          </label>
          <label>
            Password
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              autoComplete="current-password"
            />
          </label>
          {authError ? <div className="web-auth-error">{authError}</div> : null}
          <div className="web-auth-actions">
            <button type="button" disabled={busy} onClick={() => void handleLogin()}>
              Sign in
            </button>
            <button type="button" disabled={busy} onClick={() => void handleBootstrap()}>
              Bootstrap first user
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <Layout
      sidebarCollapsed={sidebarCollapsed}
      rightPanelOpen={filePanelOpen}
      rightPanelWidth={360}
      rightPanel={activeRun ? (
        <FileManager
          workspaceId={activeRun.id}
          selectedPath={selectedFilePath}
          onSelectedPathChange={setSelectedFilePath}
          onClose={() => setFilePanelOpen(false)}
          panelWidth={360}
          onPanelWidthChange={() => undefined}
          listFiles={() => client.listRunFiles(activeRun.id).then((response) => response.files)}
          readFile={(_workspaceId, path) => client.readRunFile(activeRun.id, path)}
          loadGitStatus={() =>
            client.getRunGitStatus(activeRun.id).then((response) => ({
              files: response.files.map((file) => ({
                ...file,
                additions: 0,
                deletions: 0,
              })),
            }))
          }
        />
      ) : null}
      sidebar={(
        <aside className="web-sidebar">
          <div className="web-sidebar-scroll">
            <div className="web-brand">
              <div className="web-brand-title">open-web-codex</div>
              <div className="web-brand-sub">
                {gatewayState} {gatewayVersion ? `v${gatewayVersion}` : ""}
              </div>
            </div>
            <section className="web-sidebar-section">
              <h2>Projects</h2>
              <ul className="web-list">
                {projects.map((project) => (
                  <li key={project.id}>
                    <button
                      type="button"
                      className={project.id === activeProjectId ? "is-active" : ""}
                      onClick={() => setActiveProjectId(project.id)}
                    >
                      {project.name}
                    </button>
                    <button type="button" className="web-inline-danger" onClick={() => void handleDeleteProject(project.id)}>
                      Delete
                    </button>
                  </li>
                ))}
              </ul>
              <div className="web-inline-form">
                <input
                  placeholder="Project name"
                  value={newProjectName}
                  onChange={(event) => setNewProjectName(event.target.value)}
                />
                <input
                  placeholder="Git URL"
                  value={newProjectGitUrl}
                  onChange={(event) => setNewProjectGitUrl(event.target.value)}
                />
                <button type="button" disabled={busy} onClick={() => void handleCreateProject()}>
                  Add project
                </button>
              </div>
            </section>
            <section className="web-sidebar-section">
              <h2>Tasks</h2>
              <ul className="web-list">
                {tasks.map((task) => (
                  <li key={task.id}>
                    <button
                      type="button"
                      className={task.id === activeTaskId ? "is-active" : ""}
                      onClick={() => setActiveTaskId(task.id)}
                    >
                      {task.title}
                    </button>
                  </li>
                ))}
              </ul>
              <div className="web-inline-form">
                <input
                  placeholder="Task title"
                  value={newTaskTitle}
                  onChange={(event) => setNewTaskTitle(event.target.value)}
                />
                <button type="button" disabled={busy || !activeProjectId} onClick={() => void handleCreateTask()}>
                  New task + run
                </button>
              </div>
            </section>
          </div>
          <div className="web-sidebar-bottom">
            <button type="button" onClick={() => setToken("")}>Sign out</button>
            <button type="button" onClick={() => void checkGateway()}>Refresh health</button>
          </div>
        </aside>
      )}
    >
      <section className="web-main">
        <header className="web-header">
          <button type="button" onClick={() => setSidebarCollapsed((value) => !value)}>
            Menu
          </button>
          <div className="web-header-title">
            {tasks.find((task) => task.id === activeTaskId)?.title ?? "Select a task"}
          </div>
          <div className="web-header-meta">
            {activeRun ? `run: ${activeRun.status}` : "no active run"}
          </div>
          {activeRun ? (
            <button type="button" onClick={() => setFilePanelOpen((value) => !value)}>
              Files
            </button>
          ) : null}
        </header>
        {authError ? (
          <div className="web-error-banner">
            <span>{authError}</span>
            <button type="button" onClick={() => setAuthError(null)}>Dismiss</button>
          </div>
        ) : null}
        <MessageList
          items={messages}
          thinking={thinking}
          onDecideApproval={(approvalId, decision) => void handleDecideApproval(approvalId, decision)}
        />
        <footer className="web-composer">
          <textarea
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            placeholder="Describe what Codex should do..."
            rows={3}
          />
          <div className="web-composer-actions">
            <button type="button" disabled={busy || !draft.trim()} onClick={() => void handleSend()}>
              Send
            </button>
            {activeTurnId && activeRun ? (
              <button type="button" disabled={busy} onClick={() => void handleInterrupt()}>
                Stop
              </button>
            ) : null}
          </div>
        </footer>
      </section>
    </Layout>
  );
}
