import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import Layout from "./components/Layout";
import PlatformAuthScreen from "./components/PlatformAuthScreen";
import Conversation from "./components/Conversation";
import type { ModelProviderSummary, ModelSummary } from "./components/Conversation/Composer";
import type { QueuedFollowUp } from "./components/Conversation/FollowUpQueue";
import FileManager from "./components/FileManager";
import type { MessageEntry } from "./components/Conversation/MessageList";
import { PlatformClient } from "./services/platformClient";
import type { PlatformApproval, PlatformProject, PlatformRun, PlatformRunEvent, PlatformTask } from "./services/platformTypes";
import type {
  AccessMode,
  RequestUserInputRequest,
  RequestUserInputResponse,
  ThreadTokenUsage,
} from "./types";
import {
  parseModelCatalog,
  parseModelProviderCatalog,
  readStoredModelId,
  writeStoredModelId,
} from "./utils/modelCatalog";
import {
  appendRunEvents,
  mergeProjectedMessages,
  readStoredEffort,
  runStartIdempotencyKey,
  shouldPollTaskRun,
  writeStoredEffort,
} from "./utils/platformWebAppHelpers";
import {
  latestTurnId,
  maxEventSequence,
  projectedEventsToLogEntries,
  turnStartedAtFromEvents,
} from "./utils/projectedRunEventsToLogEntries";
import { parseUserInputFromApproval } from "./utils/parseUserInputFromApproval";
import { tokenUsageFromRunEvents } from "./utils/tokenUsageFromRunEvents";
import "./styles/web.css";
import "./styles/web-refactor.css";

type GatewayState = "checking" | "online" | "offline";

const newId = () =>
  crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(36).slice(2)}`;

function isRunTerminal(status: string) {
  return status === "completed" || status === "cancelled" || status === "failed";
}

function isRunInFlight(status: string) {
  return status === "running"
    || status === "provisioning"
    || status === "queued"
    || status === "waiting_approval";
}

const EFFORT_OPTIONS = ["low", "medium", "high"] as const;

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
  const [stopping, setStopping] = useState(false);
  const [thinking, setThinking] = useState(false);
  const [turnStartedAt, setTurnStartedAt] = useState<number | null>(null);
  const [queuedFollowUps, setQueuedFollowUps] = useState<QueuedFollowUp[]>([]);
  const [steeringFollowUpId, setSteeringFollowUpId] = useState<string | null>(null);
  const [submittingUserInputId, setSubmittingUserInputId] = useState<string | number | null>(null);
  const [pendingApprovals, setPendingApprovals] = useState<PlatformApproval[]>([]);
  const [activeTurnId, setActiveTurnId] = useState<string | null>(null);
  const [filePanelOpen, setFilePanelOpen] = useState(false);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [newProjectName, setNewProjectName] = useState("");
  const [newProjectGitUrl, setNewProjectGitUrl] = useState("");
  const [newTaskTitle, setNewTaskTitle] = useState("");
  const [providers, setProviders] = useState<ModelProviderSummary[]>([]);
  const [currentProviderId, setCurrentProviderId] = useState<string | null>(null);
  const [models, setModels] = useState<ModelSummary[]>([]);
  const [selectedModelId, setSelectedModelId] = useState<string | null>(null);
  const [selectedEffort, setSelectedEffort] = useState<string>("medium");
  const [accessMode, setAccessMode] = useState<AccessMode>("current");
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [tokenUsage, setTokenUsage] = useState<ThreadTokenUsage | null>(null);
  const eventSequenceRef = useRef(0);
  const eventsRef = useRef<PlatformRunEvent[]>([]);
  const pendingUserMessagesRef = useRef<MessageEntry[]>([]);
  const activeTaskIdRef = useRef<string | null>(null);
  const syncInFlightRef = useRef(false);
  const runStartInFlightRef = useRef<Promise<PlatformRun> | null>(null);

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

  useEffect(() => {
    activeTaskIdRef.current = activeTaskId;
  }, [activeTaskId]);

  const loadProjects = useCallback(async () => {
    const next = await client.listProjects();
    setProjects(next);
    setActiveProjectId((current) => current ?? next[0]?.id ?? null);
  }, [client]);

  const loadTasks = useCallback(async (projectId: string) => {
    const next = await client.listTasks(projectId);
    setTasks(next);
    setActiveTaskId((current) => current ?? next[0]?.id ?? null);
  }, [client]);

  const refreshRunState = useCallback(async (taskId: string) => {
    const { run } = await client.getActiveRun(taskId);
    if (activeTaskIdRef.current === taskId) {
      setActiveRun(run);
    }
    return run;
  }, [client]);

  const syncTaskEvents = useCallback(async (taskId: string, run: PlatformRun | null) => {
    const events = await client.listTaskEvents(
      taskId,
      eventSequenceRef.current > 0 ? eventSequenceRef.current : undefined,
    );
    if (activeTaskIdRef.current !== taskId) {
      return;
    }
    if (events.length > 0) {
      eventsRef.current = appendRunEvents(eventsRef.current, events);
      eventSequenceRef.current = maxEventSequence(eventsRef.current);
      setActiveTurnId(latestTurnId(eventsRef.current));
      setTokenUsage(tokenUsageFromRunEvents(eventsRef.current));
      setTurnStartedAt(turnStartedAtFromEvents(eventsRef.current));
    }
    const approvals = run && (run.status === "waiting_approval" || isRunInFlight(run.status))
      ? await client.listApprovals(run.id)
      : [];
    if (activeTaskIdRef.current !== taskId) {
      return;
    }
    setPendingApprovals(approvals);
    const projected = projectedEventsToLogEntries(eventsRef.current, approvals);
    const merged = mergeProjectedMessages(projected, pendingUserMessagesRef.current);
    pendingUserMessagesRef.current = pendingUserMessagesRef.current.filter(
      (entry) => !projected.some((projectedEntry) => projectedEntry.level === "user" && projectedEntry.text.trim() === entry.text.trim()),
    );
    setMessages(merged);
    setThinking(run ? isRunInFlight(run.status) : false);
  }, [client]);

  useEffect(() => {
    if (!token) return;
    void loadProjects().catch((error) => setAuthError(String(error)));
  }, [token, loadProjects]);

  useEffect(() => {
    if (!activeProjectId || !token) return;
    setTasks([]);
    setActiveTaskId(null);
    void loadTasks(activeProjectId).catch((error) => setAuthError(String(error)));
  }, [activeProjectId, token, loadTasks]);

  useEffect(() => {
    if (!activeTaskId || !token) return;
    eventsRef.current = [];
    eventSequenceRef.current = 0;
    pendingUserMessagesRef.current = [];
    setMessages([]);
    setActiveRun(null);
    setTokenUsage(null);
    setTurnStartedAt(null);
    setQueuedFollowUps([]);
    setPendingApprovals([]);
    setSelectedModelId(readStoredModelId(activeTaskId));
    setSelectedEffort(readStoredEffort(activeTaskId) ?? "medium");
    void (async () => {
      const run = await refreshRunState(activeTaskId);
      await syncTaskEvents(activeTaskId, run);
    })().catch((error) => setAuthError(String(error)));
  }, [activeTaskId, token, refreshRunState, syncTaskEvents]);

  const refreshModelCatalog = useCallback(async (options?: { forceModels?: boolean }) => {
    if (!token) return;
    setCatalogLoading(true);
    setCatalogError(null);
    try {
      const [providerResponse, modelResponse] = await Promise.all([
        client.listModelProviders(),
        client.listModels(Boolean(options?.forceModels)),
      ]);
      const providerCatalog = parseModelProviderCatalog(providerResponse);
      setProviders(providerCatalog.providers);
      setCurrentProviderId(providerCatalog.currentProviderId);
      const nextModels = parseModelCatalog(modelResponse);
      setModels(nextModels);
      setSelectedModelId((current) => {
        if (current && nextModels.some((model) => model.id === current)) {
          return current;
        }
        const stored = activeTaskIdRef.current
          ? readStoredModelId(activeTaskIdRef.current)
          : null;
        if (stored && nextModels.some((model) => model.id === stored)) {
          return stored;
        }
        return nextModels[0]?.id ?? null;
      });
    } catch (error) {
      setCatalogError(error instanceof Error ? error.message : String(error));
    } finally {
      setCatalogLoading(false);
    }
  }, [client, token]);

  useEffect(() => {
    if (!token) return;
    void refreshModelCatalog();
  }, [token, refreshModelCatalog]);

  useEffect(() => {
    if (!activeTaskId || !token) return undefined;
    let cancelled = false;
    const tick = async () => {
      if (syncInFlightRef.current) {
        return;
      }
      const taskId = activeTaskId;
      syncInFlightRef.current = true;
      try {
        const run = await refreshRunState(taskId);
        if (cancelled || activeTaskIdRef.current !== taskId) {
          return;
        }
        if (!run || shouldPollTaskRun(run.status)) {
          await syncTaskEvents(taskId, run);
        }
      } catch (error) {
        if (activeTaskIdRef.current === taskId) {
          setAuthError(error instanceof Error ? error.message : String(error));
        }
      } finally {
        syncInFlightRef.current = false;
      }
    };
    void tick();
    const timer = window.setInterval(() => {
      void tick();
    }, 1200);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
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
      const run = await client.startTaskRun(task.id, runStartIdempotencyKey(task.id));
      if (activeTaskIdRef.current === task.id) {
        setActiveRun(run.run);
        await syncTaskEvents(task.id, run.run);
      }
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const ensureActiveRun = async (taskId: string) => {
    if (activeTaskIdRef.current !== taskId) {
      throw new Error("task changed before run could be ensured");
    }
    const { run } = await client.getActiveRun(taskId);
    if (activeTaskIdRef.current !== taskId) {
      throw new Error("task changed before run could be ensured");
    }
    if (run && !isRunTerminal(run.status)) {
      setActiveRun(run);
      return run;
    }
    if (!runStartInFlightRef.current) {
      runStartInFlightRef.current = client
        .startTaskRun(taskId, runStartIdempotencyKey(taskId))
        .then((started) => {
          if (activeTaskIdRef.current === taskId) {
            setActiveRun(started.run);
          }
          return started.run;
        })
        .finally(() => {
          runStartInFlightRef.current = null;
        });
    }
    return runStartInFlightRef.current;
  };

  const handleSend = async () => {
    const text = draft.trim();
    if (!activeTaskId || !text) return;
    if (thinking) {
      setQueuedFollowUps((current) => [...current, { id: newId(), text }]);
      setDraft("");
      return;
    }
    setBusy(true);
    try {
      const optimistic: MessageEntry = { id: newId(), level: "user", text };
      pendingUserMessagesRef.current = [...pendingUserMessagesRef.current, optimistic];
      setMessages((current) => [...current, optimistic]);
      setDraft("");
      setTurnStartedAt(Date.now());
      await ensureActiveRun(activeTaskId);
      await client.sendMessage(activeTaskId, text, {
        model: selectedModelId,
        effort: selectedEffort,
        accessMode,
      });
      const run = await refreshRunState(activeTaskId);
      await syncTaskEvents(activeTaskId, run);
    } catch (error) {
      pendingUserMessagesRef.current = pendingUserMessagesRef.current.filter((entry) => entry.text !== text);
      setTurnStartedAt(null);
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  };

  const handleSteerFollowUp = async (followUpId: string) => {
    if (!activeRun?.id || !activeTurnId || steeringFollowUpId) return;
    const item = queuedFollowUps.find((entry) => entry.id === followUpId);
    if (!item) return;
    setSteeringFollowUpId(followUpId);
    try {
      await client.steerRun(activeRun.id, activeTurnId, item.text);
      setQueuedFollowUps((current) => current.filter((entry) => entry.id !== followUpId));
      if (activeTaskId) {
        const run = await refreshRunState(activeTaskId);
        await syncTaskEvents(activeTaskId, run);
      }
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setSteeringFollowUpId(null);
    }
  };

  const handleSubmitUserInput = async (
    request: RequestUserInputRequest,
    response: RequestUserInputResponse,
  ) => {
    if (submittingUserInputId !== null) return;
    const approval = pendingApprovals.find(
      (entry) => entry.codex_request_id === String(request.request_id),
    );
    if (!approval) {
      setAuthError("No pending platform approval found for this input request.");
      return;
    }
    setSubmittingUserInputId(request.request_id);
    try {
      await client.respondToApproval(approval.id, { answers: response.answers });
      if (activeTaskId) {
        const run = await refreshRunState(activeTaskId);
        await syncTaskEvents(activeTaskId, run);
      }
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setSubmittingUserInputId(null);
    }
  };

  const openFile = (path: string) => {
    setSelectedFilePath(path);
    setFilePanelOpen(true);
  };

  const handleSelectModel = async (modelId: string) => {
    setSelectedModelId(modelId);
    if (activeTaskId) {
      writeStoredModelId(activeTaskId, modelId);
      try {
        await client.updateThreadSettings(activeTaskId, { model: modelId });
      } catch (error) {
        setCatalogError(error instanceof Error ? error.message : String(error));
      }
    }
  };

  const handleSelectEffort = async (effort: string) => {
    setSelectedEffort(effort);
    if (activeTaskId) {
      writeStoredEffort(activeTaskId, effort);
      try {
        await client.updateThreadSettings(activeTaskId, { effort });
      } catch (error) {
        setCatalogError(error instanceof Error ? error.message : String(error));
      }
    }
  };

  const handleWriteProvider = async (input: Record<string, unknown>) => {
    const response = await client.writeModelProvider(input);
    const providerCatalog = parseModelProviderCatalog(response);
    setProviders(providerCatalog.providers);
    setCurrentProviderId(providerCatalog.currentProviderId);
    const forceModels = input.action === "fetch"
      || input.action === "select"
      || (input.action === "upsert" && input.select === true);
    if (input.action === "fetch") {
      setModels(parseModelCatalog(response));
    }
    await refreshModelCatalog({ forceModels });
  };

  const handleInterrupt = async () => {
    if (!activeRun?.id || !activeTurnId || stopping) return;
    setStopping(true);
    try {
      await client.interruptRun(activeRun.id, activeTurnId);
      if (activeTaskId) {
        const run = await refreshRunState(activeTaskId);
        await syncTaskEvents(activeTaskId, run);
      }
    } catch (error) {
      setAuthError(error instanceof Error ? error.message : String(error));
    } finally {
      setStopping(false);
    }
  };

  const handleDecideApproval = async (
    approvalId: string,
    decision: "approved" | "rejected",
  ) => {
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

  const activeUserInputRequest = pendingApprovals
    .map((approval) => parseUserInputFromApproval(approval))
    .find((request): request is RequestUserInputRequest => request !== null) ?? null;
  const activeTask = tasks.find((task) => task.id === activeTaskId) ?? null;
  const threadStatus = activeRun?.status ?? "idle";

  if (!token) {
    return (
      <PlatformAuthScreen
        baseUrl={baseUrl}
        onBaseUrlChange={setBaseUrl}
        email={email}
        onEmailChange={setEmail}
        password={password}
        onPasswordChange={setPassword}
        authError={authError}
        busy={busy}
        gatewayState={gatewayState}
        gatewayVersion={gatewayVersion}
        onLogin={() => void handleLogin()}
        onBootstrap={() => void handleBootstrap()}
      />
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
        {authError ? (
          <div className="web-error-banner">
            <span>{authError}</span>
            <button type="button" onClick={() => setAuthError(null)}>Dismiss</button>
          </div>
        ) : null}
        <Conversation
          goal={null}
          workspaceName={projects.find((project) => project.id === activeProjectId)?.name ?? null}
          threadTitle={activeTask?.title ?? null}
          conversationId={activeTaskId}
          sidebarCollapsed={sidebarCollapsed}
          onToggleSidebar={() => setSidebarCollapsed((value) => !value)}
          filePanelOpen={filePanelOpen}
          onToggleFilePanel={() => setFilePanelOpen((value) => !value)}
          onOpenFile={openFile}
          tokenUsage={tokenUsage}
          threadStatus={threadStatus}
          threadSettings={null}
          providers={providers}
          currentProviderId={currentProviderId}
          models={models}
          catalogLoading={catalogLoading}
          catalogError={catalogError}
          onRefreshCatalog={() => void refreshModelCatalog({ forceModels: true })}
          onWriteProvider={handleWriteProvider}
          selectedModelId={selectedModelId}
          onSelectModel={(modelId) => void handleSelectModel(modelId)}
          effortOptions={[...EFFORT_OPTIONS]}
          selectedEffort={selectedEffort}
          onSelectEffort={(effort) => void handleSelectEffort(effort)}
          accessMode={accessMode}
          onSelectAccessMode={setAccessMode}
          messages={messages}
          workspaceId={activeRun?.id}
          thinking={thinking}
          turnStartedAt={turnStartedAt}
          draft={draft}
          onDraftChange={setDraft}
          onSend={() => void handleSend()}
          onStop={() => void handleInterrupt()}
          stopping={stopping}
          queuedFollowUps={queuedFollowUps}
          steeringFollowUpId={steeringFollowUpId}
          canSteer={Boolean(activeTurnId) && thinking && !stopping}
          onSteerFollowUp={(id) => void handleSteerFollowUp(id)}
          onDeleteFollowUp={(id) => setQueuedFollowUps((current) => current.filter((entry) => entry.id !== id))}
          userInputRequest={activeUserInputRequest}
          submittingUserInput={submittingUserInputId === activeUserInputRequest?.request_id}
          onSubmitUserInput={(request, response) => void handleSubmitUserInput(request, response)}
          busy={busy}
          sendDisabled={!draft.trim() || !activeTaskId}
          onDecideApproval={(approvalId, decision) => void handleDecideApproval(approvalId, decision)}
        />
      </section>
    </Layout>
  );
}

export { isRunInFlight, isRunTerminal };
export { mergeProjectedMessages } from "./utils/platformWebAppHelpers";
