import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import AlertCircle from "lucide-react/dist/esm/icons/alert-circle";
import BookOpen from "lucide-react/dist/esm/icons/book-open";
import CheckCircle2 from "lucide-react/dist/esm/icons/check-circle-2";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle";
import Play from "lucide-react/dist/esm/icons/play";
import RotateCw from "lucide-react/dist/esm/icons/rotate-cw";
import X from "lucide-react/dist/esm/icons/x";
import { platformClient } from "../../../browser/session";
import type {
  RunReadiness,
  RunReadinessAction,
  SupervisorPolicySummary,
  TutorialBlueprint,
  TutorialBlueprintReconcileResponse,
  TutorialBlueprintSummary,
} from "../../../browser/types";
import type { WorkspaceInfo } from "../../types";
import type { RunLaunchSelection } from "./RunLauncherDialog";

type Props = {
  workspaces: WorkspaceInfo[];
  activeWorkspaceId: string | null;
  busy: boolean;
  onEvaluateReadiness: (
    workspaceId: string,
    selection: RunLaunchSelection,
  ) => Promise<RunReadiness>;
  onStartTask: (
    workspaceId: string,
    selection: RunLaunchSelection,
    readiness: RunReadiness,
    operationId: string,
  ) => Promise<boolean>;
  onReadinessAction: (action: RunReadinessAction, workspaceId: string) => void;
  onCatalogChanged: () => void;
  onPromptReady: (
    prompt: string,
    options?: { replaceExisting?: boolean },
  ) => boolean;
  onClose: () => void;
};

const installKey = (blueprintId: string, revision: string, workspaceId: string) =>
  `${blueprintId}@${revision}:${workspaceId}`;

function idempotencyKey() {
  const randomUUID = globalThis.crypto?.randomUUID;
  return typeof randomUUID === "function"
    ? randomUUID.call(globalThis.crypto)
    : `tutorial-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function launchOperationId() {
  const randomUUID = globalThis.crypto?.randomUUID;
  return typeof randomUUID === "function"
    ? randomUUID.call(globalThis.crypto)
    : `tutorial-launch-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function readinessActionLabel(action: RunReadinessAction) {
  switch (action) {
    case "open_workspace_data":
      return "Add data";
    case "open_agent_studio":
      return "Open Agent Studio";
    case "open_provider_settings":
      return "Configure provider";
    case "open_mcp_status":
      return "View MCP status";
    case "open_maps_settings":
      return "Configure maps";
    case "retry":
      return "Check again";
  }
}

export default function LearnDialog({
  workspaces,
  activeWorkspaceId,
  busy,
  onEvaluateReadiness,
  onStartTask,
  onReadinessAction,
  onCatalogChanged,
  onPromptReady,
  onClose,
}: Props) {
  const keys = useRef(new Map<string, string>());
  const detailRequest = useRef(0);
  const reconcileRequest = useRef(0);
  const readinessRequest = useRef(0);
  const startRequest = useRef(0);
  const contextGeneration = useRef(0);
  const mounted = useRef(true);
  const startOperationId = useRef<string | null>(null);
  const operationId =
    startOperationId.current ?? launchOperationId();
  startOperationId.current = operationId;
  const dialogRef = useRef<HTMLElement | null>(null);
  const [summaries, setSummaries] = useState<TutorialBlueprintSummary[]>([]);
  const [selectedBlueprint, setSelectedBlueprint] = useState("");
  const [workspaceId, setWorkspaceId] = useState(activeWorkspaceId ?? "");
  const [blueprint, setBlueprint] = useState<TutorialBlueprint | null>(null);
  const [installation, setInstallation] =
    useState<TutorialBlueprintReconcileResponse | null>(null);
  const [readiness, setReadiness] = useState<RunReadiness | null>(null);
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [checking, setChecking] = useState(false);
  const [starting, setStarting] = useState(false);
  const [started, setStarted] = useState(false);
  const [promptPreserved, setPromptPreserved] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const selectedSummary = useMemo(
    () =>
      summaries.find(
        (summary) =>
          `${summary.blueprint_id}@${summary.revision}` === selectedBlueprint,
      ) ?? null,
    [selectedBlueprint, summaries],
  );

  const installedPolicy = useMemo<SupervisorPolicySummary | null>(() => {
    if (installation?.status !== "installed") return null;
    const selection = installation?.supervisor_policy;
    if (!selection) return null;
    return {
      ...selection,
      display_name: blueprint?.display_name ?? "Tutorial Supervisor",
      description:
        blueprint?.description ?? "Supervisor installed from a Tutorial Blueprint.",
      source: "user_release",
    };
  }, [blueprint, installation]);

  const selection = useMemo<RunLaunchSelection | null>(
    () =>
      installedPolicy
        ? { kind: "supervisor", policy: installedPolicy }
        : null,
    [installedPolicy],
  );
  const installationComplete =
    installation?.status === "installed" && selection !== null;
  const operationBusy = installing || checking || starting;
  const interactionLocked = operationBusy || started;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      contextGeneration.current += 1;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    platformClient
      .listTutorialBlueprints()
      .then((items) => {
        if (cancelled) return;
        setSummaries(items);
        setSelectedBlueprint((current) => {
          if (current && items.some(
            (item) => `${item.blueprint_id}@${item.revision}` === current,
          )) {
            return current;
          }
          const first = items[0];
          return first ? `${first.blueprint_id}@${first.revision}` : "";
        });
      })
      .catch((reason) => {
        if (!cancelled) {
          setError(
            reason instanceof Error
              ? reason.message
              : "Tutorials could not be loaded.",
          );
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!selectedSummary) {
      setBlueprint(null);
      return;
    }
    const request = ++detailRequest.current;
    setLoading(true);
    setError(null);
    setInstallation(null);
    setReadiness(null);
    setStarted(false);
    setPromptPreserved(false);
    platformClient
      .getTutorialBlueprint(
        selectedSummary.blueprint_id,
        selectedSummary.revision,
      )
      .then((detail) => {
        if (mounted.current && request === detailRequest.current) {
          setBlueprint(detail);
        }
      })
      .catch((reason) => {
        if (mounted.current && request === detailRequest.current) {
          setError(
            reason instanceof Error
              ? reason.message
              : "The tutorial definition could not be loaded.",
          );
        }
      })
      .finally(() => {
        if (mounted.current && request === detailRequest.current) {
          setLoading(false);
        }
      });
  }, [selectedSummary]);

  const resetTutorialContext = (blueprintChanged = false) => {
    contextGeneration.current += 1;
    if (blueprintChanged) {
      detailRequest.current += 1;
      setBlueprint(null);
    }
    setInstallation(null);
    setReadiness(null);
    setStarted(false);
    setInstalling(false);
    setChecking(false);
    setStarting(false);
    setError(null);
  };

  const checkReadiness = async (
    nextSelection: RunLaunchSelection | null = selection,
    expectedGeneration = contextGeneration.current,
  ) => {
    if (
      !workspaceId ||
      !nextSelection ||
      expectedGeneration !== contextGeneration.current
    ) {
      return;
    }
    const request = ++readinessRequest.current;
    const isCurrent = () =>
      mounted.current &&
      expectedGeneration === contextGeneration.current &&
      request === readinessRequest.current;
    setChecking(true);
    setError(null);
    try {
      const result = await onEvaluateReadiness(workspaceId, nextSelection);
      if (isCurrent()) {
        setReadiness(result);
      }
    } catch (reason) {
      if (isCurrent()) {
        setError(
          reason instanceof Error
            ? reason.message
            : "Readiness could not be checked.",
        );
      }
    } finally {
      if (isCurrent()) {
        setChecking(false);
      }
    }
  };

  const reconcile = async () => {
    if (!blueprint || !workspaceId) return;
    const requestedBlueprint = blueprint;
    const requestedWorkspaceId = workspaceId;
    const generation = contextGeneration.current;
    const request = ++reconcileRequest.current;
    const isCurrent = () =>
      mounted.current &&
      generation === contextGeneration.current &&
      request === reconcileRequest.current;
    setInstalling(true);
    setError(null);
    setReadiness(null);
    try {
      const key = installKey(
        requestedBlueprint.blueprint_id,
        requestedBlueprint.revision,
        requestedWorkspaceId,
      );
      let requestKey = keys.current.get(key);
      if (!requestKey) {
        requestKey = idempotencyKey();
        keys.current.set(key, requestKey);
      }
      const result = await platformClient.reconcileTutorialBlueprint(
        requestedWorkspaceId,
        requestedBlueprint.blueprint_id,
        requestedBlueprint.revision,
        requestKey,
      );
      if (!isCurrent()) return;
      if (
        result.workspace_id !== requestedWorkspaceId ||
        result.blueprint_id !== requestedBlueprint.blueprint_id ||
        result.revision !== requestedBlueprint.revision
      ) {
        throw new Error(
          "The installed tutorial did not match the selected Workspace and revision.",
        );
      }
      setInstallation(result);
      onCatalogChanged();
      if (result.status === "installed" && !result.supervisor_policy) {
        setError(
          "The installed tutorial did not return its governed Supervisor policy.",
        );
        return;
      }
      if (result.status === "installed" && result.supervisor_policy) {
        const nextSelection: RunLaunchSelection = {
          kind: "supervisor",
          policy: {
            ...result.supervisor_policy,
            display_name: requestedBlueprint.display_name,
            description: requestedBlueprint.description,
            source: "user_release",
          },
        };
        await checkReadiness(nextSelection, generation);
      }
    } catch (reason) {
      if (isCurrent()) {
        setError(
          reason instanceof Error
            ? reason.message
            : "The example could not be installed.",
        );
      }
    } finally {
      if (isCurrent()) {
        setInstalling(false);
      }
    }
  };

  const start = async () => {
    if (
      !workspaceId ||
      !selection ||
      !readiness ||
      readiness.status === "blocked" ||
      !blueprint ||
      !installationComplete ||
      !installation
    ) {
      return;
    }
    const requestedSelection = selection;
    const requestedReadiness = readiness;
    const recommendedPrompt = installation.recommended_prompt;
    const generation = contextGeneration.current;
    const request = ++startRequest.current;
    const isCurrent = () =>
      mounted.current &&
      generation === contextGeneration.current &&
      request === startRequest.current;
    setStarting(true);
    setError(null);
    try {
      if (
        await onStartTask(
          workspaceId,
          requestedSelection,
          requestedReadiness,
          operationId,
        ) &&
        isCurrent()
      ) {
        setStarted(true);
        if (onPromptReady(recommendedPrompt)) {
          onClose();
        } else {
          setPromptPreserved(true);
        }
      }
    } catch (reason) {
      if (!isCurrent()) return;
      const code = reason && typeof reason === "object" && "code" in reason
        ? String((reason as { code?: unknown }).code ?? "")
        : "";
      if (code === "readiness_changed") {
        await checkReadiness(requestedSelection, generation);
        return;
      }
      setError(
        reason instanceof Error ? reason.message : "The tutorial could not start.",
      );
    } finally {
      if (isCurrent()) {
        setStarting(false);
      }
    }
  };

  const close = () => {
    contextGeneration.current += 1;
    onClose();
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(
      dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [href], [tabindex]:not([tabindex="-1"])',
      ) ?? [],
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  return (
    <section
      ref={dialogRef}
      className="web-learn-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="web-learn-title"
      onKeyDown={handleKeyDown}
    >
      <header className="web-learn-header">
        <div className="web-learn-heading">
          <span><BookOpen size={19} aria-hidden="true" /></span>
          <div>
            <h2 id="web-learn-title">Learn</h2>
            <p>Run a governed example, then inspect and safely modify it.</p>
          </div>
        </div>
        <button type="button" aria-label="Close Learn" onClick={close}>
          <X size={17} />
        </button>
      </header>

      <div className="web-learn-body">
        <aside className="web-learn-paths" aria-label="Learning paths">
          <strong>Learning paths</strong>
          <ol>
            <li className="is-active">Run a prepared case <small>5–10 min</small></li>
            <li>Understand the run</li>
            <li>Publish a safe change</li>
            <li>Advanced Builder</li>
          </ol>
        </aside>

        <main className="web-learn-content">
          {loading && summaries.length === 0 ? (
            <div className="web-learn-state" role="status">
              <LoaderCircle size={16} aria-hidden="true" /> Loading tutorials…
            </div>
          ) : summaries.length === 0 ? (
            <div className="web-learn-state">No installable tutorial is available.</div>
          ) : (
            <>
              <div className="web-learn-controls">
                <label>
                  Tutorial
                  <select
                    autoFocus
                    value={selectedBlueprint}
                    disabled={interactionLocked || busy}
                    onChange={(event) => {
                      resetTutorialContext(true);
                      setSelectedBlueprint(event.target.value);
                    }}
                  >
                    {summaries.map((summary) => (
                      <option
                        key={`${summary.blueprint_id}@${summary.revision}`}
                        value={`${summary.blueprint_id}@${summary.revision}`}
                      >
                        {summary.display_name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  Workspace
                  <select
                    value={workspaceId}
                    disabled={interactionLocked || busy}
                    onChange={(event) => {
                      resetTutorialContext();
                      setWorkspaceId(event.target.value);
                    }}
                  >
                    <option value="">Choose a Workspace</option>
                    {workspaces.map((workspace) => (
                      <option key={workspace.id} value={workspace.id}>
                        {workspace.name}
                      </option>
                    ))}
                  </select>
                </label>
              </div>

              {blueprint ? (
                <article className="web-learn-blueprint">
                  <div>
                    <span className="web-learn-version">{blueprint.revision}</span>
                    <h3>{blueprint.display_name}</h3>
                    <p>{blueprint.description}</p>
                  </div>
                  <dl>
                    <div><dt>Dataset</dt><dd>{blueprint.dataset.display_name}</dd></div>
                    <div><dt>Agents</dt><dd>{blueprint.agent_templates.length}</dd></div>
                    <div>
                      <dt>Expected</dt>
                      <dd className="web-learn-artifact-types">
                        {blueprint.expected_artifact_types.map((artifactType) => (
                          <span key={artifactType}>{artifactType}</span>
                        ))}
                      </dd>
                    </div>
                  </dl>
                </article>
              ) : null}

              <ol className="web-learn-steps">
                <li className={workspaceId ? "is-complete" : "is-current"}>
                  <span>{workspaceId ? <CheckCircle2 size={15} /> : "1"}</span>
                  <div><strong>Choose a Workspace</strong><p>The example stays scoped to this Workspace.</p></div>
                </li>
                <li className={installationComplete ? "is-complete" : workspaceId ? "is-current" : undefined}>
                  <span>{installationComplete ? <CheckCircle2 size={15} /> : "2"}</span>
                  <div>
                    <strong>Install prepared resources</strong>
                    <p>Reconcile exact Dataset, Agent and Supervisor versions by hash.</p>
                    <button
                      type="button"
                      disabled={
                        !workspaceId ||
                        !blueprint ||
                        interactionLocked ||
                        busy
                      }
                      onClick={() => void reconcile()}
                    >
                      {installing ? <LoaderCircle size={14} aria-hidden="true" /> : null}
                      {installationComplete
                        ? "Repair example"
                        : installation?.status === "partial"
                          ? "Retry setup"
                          : "Set up example"}
                    </button>
                    {installation?.status === "partial" ? (
                      <div className="web-learn-warning" role="alert">
                        <p>
                          Setup is incomplete. Retrying continues from the
                          resources that were verified.
                        </p>
                        {installation.issues.length > 0 ? (
                          <ul>
                            {installation.issues.map((issue, index) => (
                              <li key={`${issue.code}:${index}`}>
                                {issue.message}
                              </li>
                            ))}
                          </ul>
                        ) : null}
                      </div>
                    ) : null}
                  </div>
                </li>
                <li className={readiness && readiness.status !== "blocked" ? "is-complete" : installationComplete ? "is-current" : undefined}>
                  <span>{readiness && readiness.status !== "blocked" ? <CheckCircle2 size={15} /> : "3"}</span>
                  <div>
                    <strong>Check readiness</strong>
                    <p>Provider, data, exact Releases, Runtime capabilities and MCP are checked together.</p>
                    <button
                      type="button"
                      disabled={
                        !installationComplete ||
                        !selection ||
                        interactionLocked ||
                        busy
                      }
                      onClick={() => void checkReadiness()}
                    >
                      {checking ? <LoaderCircle size={14} aria-hidden="true" /> : <RotateCw size={14} aria-hidden="true" />}
                      Check readiness
                    </button>
                    {readiness ? (
                      <div className={`web-learn-readiness is-${readiness.status}`}>
                        {readiness.status === "ready" ? (
                          <CheckCircle2 size={15} aria-hidden="true" />
                        ) : (
                          <AlertCircle size={15} aria-hidden="true" />
                        )}
                        <strong>{readiness.status}</strong>
                        {readiness.checks
                          .filter((check) => check.status !== "ready" && check.action)
                          .map((check) => (
                            <button
                              type="button"
                              key={check.code}
                              disabled={operationBusy || busy}
                              onClick={() => {
                                if (check.action === "retry") {
                                  void checkReadiness();
                                } else if (check.action) {
                                  onReadinessAction(check.action, workspaceId);
                                }
                              }}
                            >
                              {readinessActionLabel(check.action!)}
                            </button>
                          ))}
                      </div>
                    ) : null}
                  </div>
                </li>
                <li className={started ? "is-complete" : readiness && readiness.status !== "blocked" ? "is-current" : undefined}>
                  <span>{started ? <CheckCircle2 size={15} /> : "4"}</span>
                  <div>
                    <strong>Start the recommended task</strong>
                    <p>The reviewed prompt is loaded after the governed Supervisor is accepted.</p>
                    <button
                      type="button"
                      className="is-primary"
                      disabled={
                        !installationComplete ||
                        !readiness ||
                        readiness.status === "blocked" ||
                        !selection ||
                        interactionLocked ||
                        busy
                      }
                      onClick={() => void start()}
                    >
                      {starting ? <LoaderCircle size={14} aria-hidden="true" /> : <Play size={14} aria-hidden="true" />}
                      Start task
                    </button>
                  </div>
                </li>
              </ol>

              {started ? (
                <div className="web-learn-next" role="status">
                  <CheckCircle2 size={17} aria-hidden="true" />
                  <div>
                    <strong>
                      {promptPreserved
                        ? "The task started and your existing draft was kept."
                        : "The task is ready in the conversation."}
                    </strong>
                    <p>
                      {promptPreserved
                        ? "The tutorial prompt was not loaded because the composer already contains your work."
                        : "Send the prepared prompt, handle any approval card, then inspect the report, map and Artifacts. Refresh afterward to verify recovery."}
                    </p>
                    {promptPreserved && installation ? (
                      <button
                        type="button"
                        onClick={() => {
                          if (
                            onPromptReady(installation.recommended_prompt, {
                              replaceExisting: true,
                            })
                          ) {
                            onClose();
                          }
                        }}
                      >
                        Replace draft with tutorial prompt
                      </button>
                    ) : null}
                  </div>
                </div>
              ) : null}
            </>
          )}
          {error ? <div className="web-learn-error" role="alert">{error}</div> : null}
        </main>
      </div>
    </section>
  );
}
