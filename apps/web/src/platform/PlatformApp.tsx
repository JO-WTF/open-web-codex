import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";

import { PlatformClient } from "./client";
import type {
  Approval,
  Me,
  Project,
  ProviderCatalog,
  Run,
  RunEvent,
  Task,
  WorkspaceStatus,
} from "./types";
import "./platform.css";

const terminalStatuses = new Set(["completed", "cancelled", "failed"]);

function eventText(event: RunEvent) {
  const data = event.payload.data;
  const record = data && typeof data === "object" ? data as Record<string, unknown> : null;
  const itemType = event.payload.itemType;
  if (itemType === "agentMessage" && typeof record?.text === "string") return record.text;
  if (itemType === "plan" && typeof record?.text === "string") return record.text;
  if (itemType === "reasoning") {
    const summary = record?.summary;
    if (typeof summary === "string") return summary;
    if (Array.isArray(summary)) return summary.join("\n");
  }
  if (itemType === "commandExecution") {
    const command = typeof record?.command === "string" ? record.command : "Command";
    const output = typeof record?.aggregatedOutput === "string" ? record.aggregatedOutput : "";
    return `${command}${output ? `\n${output}` : ""}`;
  }
  if (event.event_type === "platform.approval.requested") return "Approval required";
  if (record && Object.keys(record).length > 0) return JSON.stringify(record, null, 2);
  return event.event_type;
}

function statusClass(status: string) {
  if (status === "running") return "is-running";
  if (status === "completed") return "is-complete";
  if (status === "failed" || status === "recovery_pending") return "is-error";
  return "";
}

export default function PlatformApp() {
  const [token, setToken] = useState(() => sessionStorage.getItem("owc.session") ?? "");
  const client = useMemo(() => new PlatformClient({ token }), [token]);
  const [me, setMe] = useState<Me | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [runs, setRuns] = useState<Run[]>([]);
  const [events, setEvents] = useState<RunEvent[]>([]);
  const [approvals, setApprovals] = useState<Approval[]>([]);
  const [providers, setProviders] = useState<ProviderCatalog | null>(null);
  const [workspace, setWorkspace] = useState<WorkspaceStatus | null>(null);
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [connection, setConnection] = useState<"connecting" | "online" | "offline" | "resync">("offline");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [commitMessage, setCommitMessage] = useState("");
  const [authMode, setAuthMode] = useState<"login" | "bootstrap">("login");

  const selectedProject = projects.find((project) => project.id === selectedProjectId) ?? null;
  const selectedTask = tasks.find((task) => task.id === selectedTaskId) ?? null;
  const selectedRun = runs.find((run) => run.id === selectedRunId) ?? runs[0] ?? null;
  const visibleEvents = selectedRun
    ? events.filter((event) => event.run_id === selectedRun.id)
    : [];

  const fail = useCallback((reason: unknown) => {
    setError(reason instanceof Error ? reason.message : String(reason));
  }, []);

  const refreshApprovals = useCallback(async () => {
    if (!token) return;
    try {
      setApprovals(await client.listApprovals());
    } catch (reason) {
      fail(reason);
    }
  }, [client, fail, token]);

  const refreshProjects = useCallback(async () => {
    const next = await client.listProjects();
    setProjects(next);
    setSelectedProjectId((current) =>
      current && next.some((project) => project.id === current) ? current : next[0]?.id ?? null,
    );
  }, [client]);

  const refreshTask = useCallback(async (taskId: string) => {
    const [nextRuns, nextEvents] = await Promise.all([
      client.listRuns(taskId),
      client.listEvents(taskId),
    ]);
    setRuns(nextRuns);
    setSelectedRunId((current) =>
      current && nextRuns.some((run) => run.id === current) ? current : nextRuns[0]?.id ?? null,
    );
    setEvents(nextEvents);
  }, [client]);

  useEffect(() => {
    if (!token) return;
    let disposed = false;
    void Promise.all([client.me(), client.listProjects(), client.listProviders(), client.listApprovals()])
      .then(([identity, nextProjects, nextProviders, nextApprovals]) => {
        if (disposed) return;
        setMe(identity);
        setProjects(nextProjects);
        setProviders(nextProviders);
        setApprovals(nextApprovals);
        setSelectedProjectId((current) => current ?? nextProjects[0]?.id ?? null);
      })
      .catch((reason) => {
        if (disposed) return;
        sessionStorage.removeItem("owc.session");
        setToken("");
        fail(reason);
      });
    return () => {
      disposed = true;
    };
  }, [client, fail, token]);

  useEffect(() => {
    if (!selectedProjectId) {
      setTasks([]);
      setSelectedTaskId(null);
      return;
    }
    void client.listTasks(selectedProjectId).then((next) => {
      setTasks(next);
      setSelectedTaskId((current) =>
        current && next.some((task) => task.id === current) ? current : next[0]?.id ?? null,
      );
    }).catch(fail);
  }, [client, fail, selectedProjectId]);

  useEffect(() => {
    if (!selectedTaskId) {
      setRuns([]);
      setEvents([]);
      return;
    }
    void refreshTask(selectedTaskId).catch(fail);
  }, [fail, refreshTask, selectedTaskId]);

  useEffect(() => {
    if (!token) return;
    return client.subscribe(
      (event) => {
        if (event.run_id !== selectedRun?.id) return;
        setEvents((current) => {
          if (current.some((entry) => entry.sequence === event.sequence)) return current;
          return [...current, event].sort((left, right) => left.sequence - right.sequence);
        });
        if (event.event_type.startsWith("platform.approval")) void refreshApprovals();
        if (selectedTaskId) void refreshTask(selectedTaskId).catch(fail);
      },
      (state) => {
        setConnection(state);
        if (state === "resync" && selectedTaskId) void refreshTask(selectedTaskId).catch(fail);
      },
    );
  }, [client, fail, refreshApprovals, refreshTask, selectedRun?.id, selectedTaskId, token]);

  useEffect(() => {
    if (!selectedRun || terminalStatuses.has(selectedRun.status)) return;
    const timer = window.setInterval(() => {
      void client.getRun(selectedRun.id).then((next) => {
        setRuns((current) => current.map((run) => run.id === next.id ? next : run));
      }).catch(fail);
    }, 1500);
    return () => window.clearInterval(timer);
  }, [client, fail, selectedRun]);

  useEffect(() => {
    setWorkspace(null);
    setSelectedPaths(new Set());
    if (!selectedRun?.workspace_id) return;
    void client.workspaceStatus(selectedRun.id).then(setWorkspace).catch(() => undefined);
  }, [client, selectedRun?.id, selectedRun?.workspace_id]);

  const authenticate = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const fields = new FormData(event.currentTarget);
    setBusy(true);
    setError(null);
    try {
      const email = String(fields.get("email") ?? "");
      const password = String(fields.get("password") ?? "");
      const session = authMode === "bootstrap"
        ? await client.bootstrap(String(fields.get("name") ?? ""), email, password)
        : await client.login(email, password);
      sessionStorage.setItem("owc.session", session.session_token);
      setToken(session.session_token);
    } catch (reason) {
      fail(reason);
    } finally {
      setBusy(false);
    }
  };

  const createProject = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const fields = new FormData(event.currentTarget);
    setBusy(true);
    try {
      const created = await client.createProject(
        String(fields.get("name") ?? ""),
        String(fields.get("gitUrl") ?? ""),
        String(fields.get("branch") ?? "main"),
      );
      await refreshProjects();
      setSelectedProjectId(created.id);
      event.currentTarget.reset();
    } catch (reason) {
      fail(reason);
    } finally {
      setBusy(false);
    }
  };

  const createTask = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedProjectId) return;
    const title = String(new FormData(event.currentTarget).get("title") ?? "");
    setBusy(true);
    try {
      const created = await client.createTask(selectedProjectId, title);
      const next = await client.listTasks(selectedProjectId);
      setTasks(next);
      setSelectedTaskId(created.id);
      event.currentTarget.reset();
    } catch (reason) {
      fail(reason);
    } finally {
      setBusy(false);
    }
  };

  const startRun = async () => {
    if (!selectedTaskId) return;
    setBusy(true);
    try {
      const { run } = await client.startRun(selectedTaskId);
      await refreshTask(selectedTaskId);
      setSelectedRunId(run.id);
    } catch (reason) {
      fail(reason);
    } finally {
      setBusy(false);
    }
  };

  const send = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedTaskId || !message.trim()) return;
    const text = message.trim();
    setMessage("");
    try {
      await client.sendMessage(selectedTaskId, text);
    } catch (reason) {
      setMessage(text);
      fail(reason);
    }
  };

  const decide = async (approval: Approval, decision: "accept" | "decline") => {
    try {
      await client.decideApproval(approval.id, decision, approval.version);
      await refreshApprovals();
    } catch (reason) {
      fail(reason);
    }
  };

  const commit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!selectedRun || selectedPaths.size === 0 || !commitMessage.trim()) return;
    setBusy(true);
    try {
      await client.commitWorkspace(selectedRun.id, [...selectedPaths], commitMessage.trim());
      setWorkspace(await client.workspaceStatus(selectedRun.id));
      setSelectedPaths(new Set());
      setCommitMessage("");
    } catch (reason) {
      fail(reason);
    } finally {
      setBusy(false);
    }
  };

  if (!token || !me) {
    return (
      <main className="auth-shell">
        <section className="auth-card">
          <p className="eyebrow">OPEN WEB CODEX</p>
          <h1>Self-hosted Codex workbench</h1>
          <p>Profiles, credentials, Runs, and Git workspaces stay behind the authenticated platform.</p>
          {error && <div className="error-banner">{error}</div>}
          <form onSubmit={authenticate}>
            {authMode === "bootstrap" && <input name="name" placeholder="Your name" required />}
            <input name="email" type="email" placeholder="Email" required />
            <input name="password" type="password" placeholder="Password" required />
            <button disabled={busy}>{authMode === "login" ? "Sign in" : "Initialize instance"}</button>
          </form>
          <button className="text-button" onClick={() => setAuthMode((mode) => mode === "login" ? "bootstrap" : "login")}>
            {authMode === "login" ? "First run? Initialize the instance" : "Already initialized? Sign in"}
          </button>
        </section>
      </main>
    );
  }

  return (
    <main className="platform-shell">
      <header className="topbar">
        <div><strong>Open Web Codex</strong><span>{me.organization_role} · {me.name}</span></div>
        <div className={`live-state ${connection}`}>{connection}</div>
        <button className="text-button" onClick={() => {
          sessionStorage.removeItem("owc.session");
          setToken("");
          setMe(null);
        }}>Sign out</button>
      </header>
      {error && <div className="error-banner dismissible">{error}<button onClick={() => setError(null)}>×</button></div>}

      <div className="workbench">
        <aside className="rail projects-rail">
          <h2>Projects</h2>
          <div className="item-list">
            {projects.map((project) => (
              <button className={project.id === selectedProjectId ? "selected" : ""} key={project.id} onClick={() => setSelectedProjectId(project.id)}>
                <strong>{project.name}</strong><small>{project.default_branch}</small>
              </button>
            ))}
          </div>
          <form className="compact-form" onSubmit={createProject}>
            <input name="name" placeholder="Project name" required />
            <input name="gitUrl" placeholder="https://… or git@…" required />
            <input name="branch" placeholder="main" defaultValue="main" required />
            <button disabled={busy}>Add project</button>
          </form>
          <h2>Providers</h2>
          <div className="item-list small">
            {providers?.data.map((provider) => (
              <button key={provider.id} className={provider.isCurrent ? "selected" : ""} onClick={() => {
                void client.selectProvider(provider.id).then(setProviders).catch(fail);
              }}>
                <strong>{provider.name}</strong><small>{provider.wireApi} · {provider.modelCount} models</small>
              </button>
            ))}
          </div>
        </aside>

        <aside className="rail tasks-rail">
          <h2>{selectedProject?.name ?? "Tasks"}</h2>
          <div className="item-list">
            {tasks.map((task) => (
              <button className={task.id === selectedTaskId ? "selected" : ""} key={task.id} onClick={() => setSelectedTaskId(task.id)}>
                <strong>{task.title}</strong><small>{task.status}</small>
              </button>
            ))}
          </div>
          {selectedProjectId && <form className="compact-form" onSubmit={createTask}>
            <input name="title" placeholder="New coding task" required />
            <button disabled={busy}>Create task</button>
          </form>}
          <div className="section-heading"><h2>Runs</h2><button disabled={!selectedTaskId || busy} onClick={startRun}>Start</button></div>
          <div className="item-list small">
            {runs.map((run) => (
              <button className={run.id === selectedRun?.id ? "selected" : ""} key={run.id} onClick={() => setSelectedRunId(run.id)}>
                <strong>Attempt {run.attempt || "queued"}</strong><small className={statusClass(run.status)}>{run.status}</small>
              </button>
            ))}
          </div>
          {selectedRun && !terminalStatuses.has(selectedRun.status) && <button className="danger" onClick={() => {
            void client.cancelRun(selectedRun.id).then((next) => {
              setRuns((current) => current.map((run) => run.id === next.id ? next : run));
            }).catch(fail);
          }}>Cancel run</button>}
        </aside>

        <section className="conversation-panel">
          <div className="conversation-header">
            <div><h1>{selectedTask?.title ?? "Select a task"}</h1><span>{selectedRun?.source_ref ?? "No active Run"}</span></div>
            <span className={`status-pill ${statusClass(selectedRun?.status ?? "")}`}>{selectedRun?.status ?? "idle"}</span>
          </div>
          <div className="event-stream">
            {visibleEvents.length === 0 && <div className="empty-state">Start a Run and send a message. Durable events will appear here.</div>}
            {visibleEvents.map((event) => (
              <article key={event.id} className={`event-card ${event.payload.itemType ?? "event"}`}>
                <header><span>{event.payload.itemType ?? event.event_type}</span><time>{new Date(event.created_at).toLocaleTimeString()}</time></header>
                <pre>{eventText(event)}</pre>
              </article>
            ))}
          </div>
          <form className="composer" onSubmit={send}>
            <textarea value={message} onChange={(event) => setMessage(event.target.value)} placeholder="Ask Codex to work in this isolated Run workspace…" />
            <button disabled={selectedRun?.status !== "running" || !message.trim()}>Send</button>
          </form>
        </section>

        <aside className="rail details-rail">
          <h2>Approvals</h2>
          {approvals.length === 0 && <p className="muted">No pending approvals.</p>}
          {approvals.map((approval) => (
            <article className="approval-card" key={approval.id}>
              <strong>{approval.requestType}</strong>
              {approval.command && <code>{approval.command}</code>}
              {approval.reason && <p>{approval.reason}</p>}
              <div><button onClick={() => void decide(approval, "accept")}>Approve</button><button className="danger" onClick={() => void decide(approval, "decline")}>Decline</button></div>
            </article>
          ))}
          <div className="section-heading"><h2>Git workspace</h2><button disabled={!selectedRun?.workspace_id} onClick={() => {
            if (selectedRun) void client.workspaceStatus(selectedRun.id).then(setWorkspace).catch(fail);
          }}>Refresh</button></div>
          {workspace && <>
            <p className="branch-line">{workspace.branch}<code>{workspace.head_commit.slice(0, 10)}</code></p>
            <div className="change-list">
              {workspace.changes.map((change) => <label key={change.path}>
                <input type="checkbox" checked={selectedPaths.has(change.path)} onChange={() => setSelectedPaths((current) => {
                  const next = new Set(current);
                  if (next.has(change.path)) next.delete(change.path); else next.add(change.path);
                  return next;
                })} />
                <span>{change.path}</span><small>{change.status}</small>
              </label>)}
            </div>
            <form className="compact-form" onSubmit={commit}>
              <textarea value={commitMessage} onChange={(event) => setCommitMessage(event.target.value)} placeholder="Commit message" />
              <button disabled={busy || selectedPaths.size === 0 || !commitMessage.trim()}>Commit selected</button>
            </form>
          </>}
        </aside>
      </div>
    </main>
  );
}
