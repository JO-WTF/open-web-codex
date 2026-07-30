import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from "react";
import AlertCircle from "lucide-react/dist/esm/icons/alert-circle";
import Bot from "lucide-react/dist/esm/icons/bot";
import CheckCircle2 from "lucide-react/dist/esm/icons/check-circle-2";
import CircleDashed from "lucide-react/dist/esm/icons/circle-dashed";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle";
import MessageSquare from "lucide-react/dist/esm/icons/message-square";
import Sparkles from "lucide-react/dist/esm/icons/sparkles";
import X from "lucide-react/dist/esm/icons/x";
import type {
  AgentDefinitionSummary,
  RunReadiness,
  RunReadinessAction,
  SupervisorPolicySummary,
} from "../../../browser/types";

export type RunLaunchSelection =
  | { kind: "standard" }
  | { kind: "agent"; agent: AgentDefinitionSummary }
  | { kind: "supervisor"; policy: SupervisorPolicySummary };

type Props = {
  workspaceId: string;
  workspaceName: string;
  agents: AgentDefinitionSummary[];
  agentsLoading: boolean;
  agentsError: string | null;
  supervisorPolicies: SupervisorPolicySummary[];
  supervisorPoliciesLoading: boolean;
  supervisorPoliciesError: string | null;
  busy: boolean;
  onEvaluate: (selection: RunLaunchSelection) => Promise<RunReadiness>;
  onStart: (
    selection: RunLaunchSelection,
    readiness: RunReadiness,
    operationId: string,
  ) => Promise<boolean>;
  onAction: (action: RunReadinessAction) => void;
  onClose: () => void;
};

type LaunchMode = RunLaunchSelection["kind"];

function createLaunchOperationId() {
  const randomUUID = globalThis.crypto?.randomUUID;
  return typeof randomUUID === "function"
    ? randomUUID.call(globalThis.crypto)
    : `run-launch-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function actionLabel(action: RunReadinessAction) {
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

function statusIcon(status: RunReadiness["status"]) {
  if (status === "ready") {
    return <CheckCircle2 size={15} aria-hidden="true" />;
  }
  if (status === "degraded") {
    return <AlertCircle size={15} aria-hidden="true" />;
  }
  return <AlertCircle size={15} aria-hidden="true" />;
}

export default function RunLauncherDialog({
  workspaceId,
  workspaceName,
  agents,
  agentsLoading,
  agentsError,
  supervisorPolicies,
  supervisorPoliciesLoading,
  supervisorPoliciesError,
  busy,
  onEvaluate,
  onStart,
  onAction,
  onClose,
}: Props) {
  const dialogRef = useRef<HTMLElement | null>(null);
  const evaluationRequest = useRef(0);
  const launchOperationId = useRef<string | null>(null);
  const operationId =
    launchOperationId.current ?? createLaunchOperationId();
  launchOperationId.current = operationId;
  const [mode, setMode] = useState<LaunchMode | null>(null);
  const [selection, setSelection] = useState<RunLaunchSelection | null>(null);
  const [readiness, setReadiness] = useState<RunReadiness | null>(null);
  const [checking, setChecking] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const compatibleAgents = agents.filter(
    (agent) =>
      agent.required_workspace_id === null ||
      agent.required_workspace_id === workspaceId,
  );

  const evaluate = async (nextSelection: RunLaunchSelection) => {
    const request = ++evaluationRequest.current;
    setSelection(nextSelection);
    setReadiness(null);
    setChecking(true);
    setError(null);
    try {
      const result = await onEvaluate(nextSelection);
      if (request === evaluationRequest.current) {
        setReadiness(result);
      }
    } catch (reason) {
      if (request === evaluationRequest.current) {
        setError(
          reason instanceof Error
            ? reason.message
            : "Readiness could not be checked.",
        );
      }
    } finally {
      if (request === evaluationRequest.current) {
        setChecking(false);
      }
    }
  };

  useEffect(
    () => () => {
      evaluationRequest.current += 1;
    },
    [],
  );

  const chooseMode = (nextMode: LaunchMode) => {
    if (starting || busy) return;
    evaluationRequest.current += 1;
    setMode(nextMode);
    setSelection(null);
    setReadiness(null);
    setError(null);
    setChecking(false);
    if (nextMode === "standard") {
      void evaluate({ kind: "standard" });
    }
  };

  const start = async () => {
    if (
      !selection ||
      !readiness ||
      readiness.status === "blocked" ||
      starting ||
      busy
    ) {
      return;
    }
    setStarting(true);
    setError(null);
    try {
      if (
        await onStart(
          selection,
          readiness,
          operationId,
        )
      ) {
        onClose();
      }
    } catch (reason) {
      const code = reason && typeof reason === "object" && "code" in reason
        ? String((reason as { code?: unknown }).code ?? "")
        : "";
      if (code === "readiness_changed") {
        setError("Requirements changed before the task was accepted. Readiness was checked again.");
        await evaluate(selection);
        return;
      }
      setReadiness(null);
      setError(
        reason instanceof Error ? reason.message : "The task could not start.",
      );
    } finally {
      setStarting(false);
    }
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(
      dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
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
      className="web-run-launcher-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="web-run-launcher-title"
      aria-describedby="web-run-launcher-description"
      onKeyDown={handleKeyDown}
    >
      <header className="web-run-launcher-header">
        <div>
          <h2 id="web-run-launcher-title">Start a task</h2>
          <p id="web-run-launcher-description">
            Choose how Codex should work in {workspaceName}, then resolve any
            missing requirements before starting.
          </p>
        </div>
        <button
          type="button"
          aria-label="Close task launcher"
          onClick={onClose}
        >
          <X size={17} />
        </button>
      </header>

      <div className="web-run-launcher-body">
        <section className="web-run-launcher-section">
          <h3>Mode</h3>
          <div className="web-run-launcher-modes">
            <button
              type="button"
              aria-label="Standard"
              className={mode === "standard" ? "is-active" : undefined}
              autoFocus
              disabled={starting || busy}
              onClick={() => chooseMode("standard")}
            >
              <MessageSquare size={17} aria-hidden="true" />
              <span>
                <strong>Standard</strong>
                <small>Start a general Codex Thread.</small>
              </span>
            </button>
            <button
              type="button"
              aria-label="Agent"
              className={mode === "agent" ? "is-active" : undefined}
              disabled={starting || busy}
              onClick={() => chooseMode("agent")}
            >
              <Bot size={17} aria-hidden="true" />
              <span>
                <strong>Agent</strong>
                <small>Use one exact governed Agent Release.</small>
              </span>
            </button>
            <button
              type="button"
              aria-label="Supervisor"
              className={mode === "supervisor" ? "is-active" : undefined}
              disabled={starting || busy}
              onClick={() => chooseMode("supervisor")}
            >
              <Sparkles size={17} aria-hidden="true" />
              <span>
                <strong>Supervisor</strong>
                <small>Coordinate governed Agents dynamically.</small>
              </span>
            </button>
          </div>
        </section>

        {mode === "agent" ? (
          <section className="web-run-launcher-section">
            <h3>Agent Release</h3>
            <div className="web-run-launcher-options">
              {agentsLoading ? (
                <div className="web-run-launcher-empty" role="status">
                  Loading Agents…
                </div>
              ) : agentsError ? (
                <div className="web-run-launcher-empty" role="alert">
                  {agentsError}
                </div>
              ) : compatibleAgents.length === 0 ? (
                <div className="web-run-launcher-empty">
                  No published Agent is compatible with this Workspace.
                </div>
              ) : (
                compatibleAgents.map((agent) => (
                  <button
                    type="button"
                    className={
                      selection?.kind === "agent" &&
                      selection.agent.definition_id === agent.definition_id &&
                      selection.agent.version === agent.version &&
                      selection.agent.release_id === agent.release_id
                        ? "is-active"
                        : undefined
                    }
                    key={
                      agent.release_id ??
                      `repository:${agent.definition_id}@${agent.version}`
                    }
                    disabled={starting || busy}
                    onClick={() => void evaluate({ kind: "agent", agent })}
                  >
                    <strong>{agent.display_name}</strong>
                    <span>{agent.version}</span>
                    <small>{agent.description}</small>
                  </button>
                ))
              )}
            </div>
          </section>
        ) : null}

        {mode === "supervisor" ? (
          <section className="web-run-launcher-section">
            <h3>Supervisor Release</h3>
            <div className="web-run-launcher-options">
              {supervisorPoliciesLoading ? (
                <div className="web-run-launcher-empty" role="status">
                  Loading Supervisors…
                </div>
              ) : supervisorPoliciesError ? (
                <div className="web-run-launcher-empty" role="alert">
                  {supervisorPoliciesError}
                </div>
              ) : supervisorPolicies.length === 0 ? (
                <div className="web-run-launcher-empty">
                  No Supervisor Release is available.
                </div>
              ) : (
                supervisorPolicies.map((policy) => (
                  <button
                    type="button"
                    className={
                      selection?.kind === "supervisor" &&
                      selection.policy.policy_id === policy.policy_id &&
                      selection.policy.version === policy.version
                        ? "is-active"
                        : undefined
                    }
                    key={`${policy.policy_id}@${policy.version}`}
                    disabled={starting || busy}
                    onClick={() =>
                      void evaluate({ kind: "supervisor", policy })
                    }
                  >
                    <strong>{policy.display_name}</strong>
                    <span>{policy.version}</span>
                    <small>{policy.description}</small>
                  </button>
                ))
              )}
            </div>
          </section>
        ) : null}

        <section
          className="web-run-launcher-section web-run-readiness"
          aria-live="polite"
        >
          <h3>Readiness</h3>
          {!selection ? (
            <div className="web-run-launcher-empty">
              Choose a mode and an exact Release to check requirements.
            </div>
          ) : checking ? (
            <div className="web-run-readiness-summary is-checking" role="status">
              <LoaderCircle size={15} aria-hidden="true" />
              Checking authoritative platform and Runtime requirements…
            </div>
          ) : readiness ? (
            <>
              <div
                className={`web-run-readiness-summary is-${readiness.status}`}
              >
                {statusIcon(readiness.status)}
                <strong>
                  {readiness.status === "ready"
                    ? "Ready to start"
                    : readiness.status === "degraded"
                      ? "Ready with limitations"
                      : "Setup required"}
                </strong>
              </div>
              <div className="web-run-readiness-checks">
                {readiness.checks.map((check) => (
                  <div
                    className={`web-run-readiness-check is-${check.status}`}
                    key={check.code}
                  >
                    <span>
                      {check.status === "ready" ? (
                        <CheckCircle2 size={14} aria-hidden="true" />
                      ) : check.status === "degraded" ? (
                        <AlertCircle size={14} aria-hidden="true" />
                      ) : (
                        <CircleDashed size={14} aria-hidden="true" />
                      )}
                    </span>
                    <p>{check.message}</p>
                    {check.action ? (
                      <button
                        type="button"
                        disabled={starting || busy}
                        onClick={() => {
                          if (check.action === "retry") {
                            void evaluate(selection);
                          } else {
                            onAction(check.action!);
                          }
                        }}
                      >
                        {actionLabel(check.action)}
                      </button>
                    ) : null}
                  </div>
                ))}
              </div>
            </>
          ) : error ? (
            <div className="web-run-launcher-error" role="alert">
              {error}
              <button
                type="button"
                onClick={() => selection && void evaluate(selection)}
              >
                Check again
              </button>
            </div>
          ) : null}
        </section>
      </div>

      <footer className="web-run-launcher-actions">
        <button type="button" onClick={onClose}>
          Cancel
        </button>
        <button
          type="button"
          className="is-primary"
          disabled={
            busy ||
            starting ||
            checking ||
            !selection ||
            !readiness ||
            readiness.status === "blocked"
          }
          onClick={() => void start()}
        >
          {starting ? (
            <>
              <LoaderCircle size={14} aria-hidden="true" />
              Starting…
            </>
          ) : (
            "Start task"
          )}
        </button>
      </footer>
    </section>
  );
}
