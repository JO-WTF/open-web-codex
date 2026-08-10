import { useEffect, useState } from "react";
import Bot from "lucide-react/dist/esm/icons/bot";
import FileCheck2 from "lucide-react/dist/esm/icons/file-check-2";
import History from "lucide-react/dist/esm/icons/history";
import LoaderCircle from "lucide-react/dist/esm/icons/loader-circle";
import Network from "lucide-react/dist/esm/icons/network";
import ShieldCheck from "lucide-react/dist/esm/icons/shield-check";
import X from "lucide-react/dist/esm/icons/x";
import type {
  ArtifactSummary,
  RuntimeAgentActivity,
  RuntimeAgentActivitySubject,
  RuntimeAgentExecution,
  RuntimeAgentProjection,
  ThreadHistoryTurn,
} from "../../../browser/types";
import { ModalShell } from "../../features/design-system/components/modal/ModalShell";
import TaskApprovalQueue, {
  type TaskApprovalRequest,
} from "./TaskApprovalQueue";

type Props = {
  taskTitle: string;
  agents: RuntimeAgentProjection[];
  activities?: RuntimeAgentActivity[];
  executions?: RuntimeAgentExecution[];
  artifacts: ArtifactSummary[];
  approvals?: TaskApprovalRequest[];
  onResolveApproval?: (
    workspaceId: string,
    requestId: number | string,
    decision: "accept" | "decline",
  ) => void;
  onLoadAgentHistory?: (threadId: string) => Promise<ThreadHistoryTurn[]>;
  onLoadArtifactContent?: (artifactId: string) => Promise<Record<string, unknown>>;
  loading?: boolean;
  error?: string | null;
};

type StatusTone = "idle" | "active" | "waiting" | "terminal" | "error";

type AgentExecutionStatus = RuntimeAgentExecution["status"];
type AgentActivityStatus = RuntimeAgentActivity["status"];

type ReviewState =
  | {
      kind: "agent";
      resourceId: string;
      title: string;
      loading: boolean;
      error: string | null;
      turns: ThreadHistoryTurn[];
    }
  | {
      kind: "artifact";
      resourceId: string;
      title: string;
      loading: boolean;
      error: string | null;
      content: Record<string, unknown> | null;
    };

const activityTimeFormatter = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
});

function agentLabel(agent: RuntimeAgentProjection): string {
  if (agent.is_root) return "Root Supervisor";
  return agent.agent_nickname?.trim()
    || agent.agent_role?.trim()
    || "Runtime Agent";
}

function agentStatusPresentation(agent: RuntimeAgentProjection): {
  label: string;
  tone: StatusTone;
} {
  const flags = new Set(agent.active_flags);
  if ([...flags].some((flag) => flag.toLowerCase().includes("waiting"))) {
    return { label: "Waiting", tone: "waiting" };
  }
  switch (agent.status_type) {
    case "active":
    case "running":
    case "inProgress":
      return { label: "Running", tone: "active" };
    case "completed":
      return { label: "Completed", tone: "terminal" };
    case "failed":
    case "error":
    case "systemError":
      return { label: "Failed", tone: "error" };
    case "interrupted":
      return { label: "Interrupted", tone: "terminal" };
    case "idle":
      return { label: "Ready", tone: "idle" };
    default:
      return { label: "Unknown", tone: "idle" };
  }
}

function executionStatusPresentation(status: AgentExecutionStatus): {
  label: string;
  tone: StatusTone;
} {
  switch (status) {
    case "pending":
      return { label: "Queued", tone: "idle" };
    case "running":
      return { label: "Running", tone: "active" };
    case "waiting":
      return { label: "Waiting", tone: "waiting" };
    case "waiting_for_input":
      return { label: "Waiting for your input", tone: "waiting" };
    case "completed":
      return { label: "Completed", tone: "terminal" };
    case "failed":
      return { label: "Failed", tone: "error" };
    case "rejected":
      return { label: "Rejected", tone: "error" };
    case "cancelled":
      return { label: "Cancelled", tone: "terminal" };
    case "timeout":
      return { label: "Timed out", tone: "error" };
    case "interrupted":
      return { label: "Interrupted", tone: "terminal" };
  }
}

function activityStatusPresentation(status: AgentActivityStatus): {
  label: string;
  tone: StatusTone;
} {
  switch (status) {
    case "pending":
      return { label: "Queued", tone: "idle" };
    case "running":
      return { label: "Running", tone: "active" };
    case "waiting":
      return { label: "Waiting", tone: "waiting" };
    case "completed":
      return { label: "Completed", tone: "terminal" };
    case "failed":
      return { label: "Failed", tone: "error" };
  }
}

function activitySubjectAction(
  subject: RuntimeAgentActivitySubject | null | undefined,
): string | null {
  if (!subject) return null;

  switch (subject.kind) {
    case "mcp_tool": {
      const server = subject.server?.trim();
      const tool = subject.tool?.trim();
      return [server, tool].filter(Boolean).join(" · ") || "MCP tool";
    }
    case "runtime_tool": {
      const namespace = subject.namespace?.trim();
      const tool = subject.tool?.trim();
      return [namespace, tool].filter(Boolean).join(" · ") || "Runtime tool";
    }
    case "workspace_action": {
      const action = subject.action.trim();
      const path = subject.path?.trim();
      // `workspace_command` is the server's bounded unknown-action marker. It
      // is deliberately rendered as the server's safe neutral label instead
      // of reviving the old generic "Using/Completed ..." timeline copy.
      const safeAction = action && action !== "workspace_command"
        ? action
        : "workspace operation";
      return path ? `${safeAction} · ${path}` : safeAction;
    }
    case "web_search":
      return "web search";
    case "image_view":
      return "image inspection";
    case "image_generation":
      return "image generation";
  }
}

function activityAction(activity: RuntimeAgentActivity): string {
  return activitySubjectAction(activity.subject)
    ?? (activity.title.trim() || activityKindLabel(activity.kind));
}

function activityIdentity(activity: RuntimeAgentActivity): string {
  return [
    activity.run_id,
    activity.thread_id,
    activity.turn_id ?? "",
    activity.item_id ?? "",
    String(activity.sequence),
  ].join("\u001f");
}

function activitySortKey(activity: RuntimeAgentActivity): string {
  return [
    activityIdentity(activity),
    activity.created_at,
    activity.kind,
    activity.status,
    activity.title,
    activity.detail ?? "",
  ].join("\u001f");
}

/**
 * Runtime history and live projections can contain the same official Item.
 * Keep this a pure projection helper: no component cache, event synthesis or
 * terminal-state rewriting is allowed here.
 */
export function orderAndDedupeActivities(
  activities: RuntimeAgentActivity[],
): RuntimeAgentActivity[] {
  const seen = new Set<string>();
  return [...activities]
    .sort((left, right) => {
      const sequenceOrder = left.sequence - right.sequence;
      if (sequenceOrder !== 0) return sequenceOrder;
      const leftKey = activitySortKey(left);
      const rightKey = activitySortKey(right);
      return leftKey < rightKey ? -1 : leftKey > rightKey ? 1 : 0;
    })
    .filter((activity) => {
      const identity = activityIdentity(activity);
      if (seen.has(identity)) return false;
      seen.add(identity);
      return true;
    });
}

function activityKindLabel(kind: RuntimeAgentActivity["kind"]): string {
  switch (kind) {
    case "assignment":
      return "Assignment";
    case "guidance":
      return "Guidance";
    case "turn_started":
    case "turn_completed":
      return "Work cycle";
    case "tool_started":
    case "tool_completed":
    case "tool_failed":
      return "Tool";
    case "reporting":
      return "Progress";
    case "waiting":
      return "Wait";
    case "input_requested":
      return "Input";
    case "input_answered":
      return "Input resolved";
    case "completed":
      return "Completion";
    case "failed":
      return "Failure";
    case "rejected":
      return "Rejection";
    case "cancelled":
      return "Cancellation";
    case "timeout":
      return "Timeout";
    case "interrupted":
      return "Interruption";
  }
}

function activityTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "";
  }
  return activityTimeFormatter.format(date);
}

function StatusBadge({ label, tone }: { label: string; tone: StatusTone }) {
  return (
    <span className={`web-supervisor-agent-status is-${tone}`}>
      <span aria-hidden="true" />
      {label}
    </span>
  );
}

function Detail({
  label,
  children,
}: {
  label: string;
  children: string;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function reviewItemText(item: Record<string, unknown>): string {
  const text = typeof item.text === "string" ? item.text.trim() : "";
  if (text) return text;
  const type = typeof item.type === "string" ? item.type : "Runtime item";
  const server = typeof item.server === "string" ? item.server : "";
  const tool = typeof item.tool === "string" ? item.tool : "";
  const command = typeof item.command === "string" ? item.command : "";
  if (server && tool) return `${server} · ${tool}`;
  if (tool) return tool;
  if (command) return command;
  return type;
}

export default function SupervisorOverview({
  taskTitle,
  agents,
  activities = [],
  executions = [],
  artifacts,
  approvals = [],
  onResolveApproval,
  onLoadAgentHistory,
  onLoadArtifactContent,
  loading = false,
  error = null,
}: Props) {
  const [review, setReview] = useState<ReviewState | null>(null);
  const rootAgent = agents.find((agent) => agent.is_root) ?? null;
  const orderedActivities = orderAndDedupeActivities(activities);
  const rootActivities = orderedActivities
    .filter((activity) => rootAgent && activity.thread_id === rootAgent.thread_id);
  const rootBehavior = [...rootActivities]
    .reverse()
    .find((activity) =>
      activity.kind !== "assignment"
      && activity.kind !== "reporting");
  const rootProgress = [...rootActivities]
    .reverse()
    .find((activity) => activity.kind === "reporting");
  const agentsByThread = new Map(agents.map((agent) => [agent.thread_id, agent]));
  const timelineActivities = orderedActivities;
  const rootStatus = rootAgent
    ? agentStatusPresentation(rootAgent)
    : { label: "Starting", tone: "idle" as const };

  useEffect(() => {
    if (!review) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setReview(null);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [review]);

  const openAgentHistory = async (
    execution: RuntimeAgentExecution,
    agent: RuntimeAgentProjection | undefined,
  ) => {
    if (!onLoadAgentHistory) return;
    const title = `${agent ? agentLabel(agent) : "Runtime Agent"} history`;
    setReview({
      kind: "agent",
      resourceId: execution.thread_id,
      title,
      loading: true,
      error: null,
      turns: [],
    });
    try {
      const turns = await onLoadAgentHistory(execution.thread_id);
      setReview((current) =>
        current?.kind === "agent" && current.resourceId === execution.thread_id
        ? { ...current, loading: false, turns }
        : current);
    } catch (loadError) {
      setReview((current) =>
        current?.kind === "agent" && current.resourceId === execution.thread_id
        ? {
            ...current,
            loading: false,
            error: loadError instanceof Error
              ? loadError.message
              : "Agent history could not be loaded.",
          }
        : current);
    }
  };

  const openArtifactContent = async (artifact: ArtifactSummary) => {
    if (!onLoadArtifactContent || artifact.state !== "ready") return;
    const title = artifact.display_name || artifact.artifact_schema;
    setReview({
      kind: "artifact",
      resourceId: artifact.id,
      title,
      loading: true,
      error: null,
      content: null,
    });
    try {
      const content = await onLoadArtifactContent(artifact.id);
      setReview((current) =>
        current?.kind === "artifact" && current.resourceId === artifact.id
        ? { ...current, loading: false, content }
        : current);
    } catch (loadError) {
      setReview((current) =>
        current?.kind === "artifact" && current.resourceId === artifact.id
        ? {
            ...current,
            loading: false,
            error: loadError instanceof Error
              ? loadError.message
              : "Artifact content could not be loaded.",
          }
        : current);
    }
  };

  return (
    <>
      <section className="web-supervisor-overview" aria-label="Enterprise Supervisor collaboration">
      <div className="web-supervisor-overview-heading">
        <span className="web-supervisor-overview-icon" aria-hidden="true">
          <ShieldCheck size={16} />
        </span>
        <div>
          <strong>Agent collaboration</strong>
          <span>Runtime-owned Agent collaboration</span>
        </div>
      </div>

      {error ? (
        <p className="web-supervisor-overview-error" role="alert">{error}</p>
      ) : loading && !rootAgent ? (
        <p className="web-supervisor-overview-empty" role="status">
          Loading collaboration activity…
        </p>
      ) : agents.length === 0 ? (
        <div className="web-supervisor-empty-state" role="status">
          <span aria-hidden="true">
            <Bot size={20} />
          </span>
          <strong>No Agent activity yet</strong>
          <p>Runtime Agents and their task progress will appear here when collaboration starts.</p>
        </div>
      ) : (
        <>
          {rootAgent ? (
            <article className="web-supervisor-agent web-supervisor-root" aria-label="Supervisor status">
              <div className="web-supervisor-agent-summary">
                <span className="web-supervisor-agent-icon" aria-hidden="true">
                  <Bot size={15} />
                </span>
                <span className="web-supervisor-agent-copy">
                  <strong>{agentLabel(rootAgent)}</strong>
                  <span>Supervisor · pinned</span>
                </span>
                <StatusBadge {...rootStatus} />
              </div>
              <dl className="web-supervisor-agent-details is-supervisor">
                <Detail label="Current task">{taskTitle}</Detail>
                <Detail label="Run status">{rootStatus.label}</Detail>
                <Detail label="Current behavior">
                  {rootBehavior?.detail
                    ?? (rootBehavior ? activityAction(rootBehavior) : null)
                    ?? "Coordinating the collaboration"}
                </Detail>
                <Detail label="Latest progress">
                  {rootProgress?.detail
                    ?? (rootProgress ? activityAction(rootProgress) : null)
                    ?? "No progress reported yet"}
                </Detail>
              </dl>
            </article>
          ) : null}

          <TaskApprovalQueue
            approvals={approvals}
            ariaLabel="Agent approvals"
            onResolve={onResolveApproval}
          />

          <div className="web-supervisor-executions" aria-label="Agent task stream">
            <div className="web-supervisor-section-heading">
              <Network size={14} aria-hidden="true" />
              <strong>Agent task stream</strong>
              <span>{executions.length}</span>
            </div>
            {executions.length ? (
              <ol className="web-supervisor-task-stream">
                {executions.map((execution) => {
                  const status = executionStatusPresentation(execution.status);
                  const agent = agentsByThread.get(execution.thread_id);
                  return (
                    <li key={execution.id}>
                      <span className={`web-supervisor-stream-node is-${status.tone}`} aria-hidden="true" />
                      <article className={`web-supervisor-agent is-execution is-${execution.status}`}>
                        <div className="web-supervisor-agent-summary">
                          <span className="web-supervisor-agent-icon" aria-hidden="true">
                            <Network size={15} />
                          </span>
                          <span className="web-supervisor-agent-copy">
                            <strong>{execution.display_title || (agent ? agentLabel(agent) : "Runtime Agent")}</strong>
                            <span>
                              {agent?.agent_role ?? "Runtime Agent"}
                              {" · "}
                              Task {execution.ordinal}
                            </span>
                          </span>
                          <StatusBadge {...status} />
                        </div>
                        <dl className="web-supervisor-agent-details">
                          <Detail label="Current task">
                            {execution.task ?? "Waiting for Supervisor assignment details"}
                          </Detail>
                          <Detail label="Task status">{status.label}</Detail>
                          <Detail label="Current behavior">{execution.current_behavior}</Detail>
                          <Detail label="Latest progress">
                            {execution.latest_progress ?? "No progress reported yet"}
                          </Detail>
                          {execution.wait_cycle_count > 0 ? (
                            <Detail label="Wait cycles">{String(execution.wait_cycle_count)}</Detail>
                          ) : null}
                          {execution.result_summary ? (
                            <Detail label="Result">{execution.result_summary}</Detail>
                          ) : null}
                        </dl>
                        {onLoadAgentHistory ? (
                          <button
                            type="button"
                            className="web-supervisor-review-action"
                            onClick={() => void openAgentHistory(execution, agent)}
                          >
                            Review Agent history
                          </button>
                        ) : null}
                      </article>
                    </li>
                  );
                })}
              </ol>
            ) : (
              <p className="web-supervisor-overview-empty">
                Waiting for the Supervisor to assign work.
              </p>
            )}
          </div>

          <div className="web-supervisor-activity" aria-label="Agent behavior log">
            <div className="web-supervisor-section-heading">
              <History size={14} aria-hidden="true" />
              <strong>Agent behavior log</strong>
              <span>{timelineActivities.length}</span>
            </div>
            {timelineActivities.length ? (
              <ol>
                {timelineActivities.map((activity) => {
                  const status = activityStatusPresentation(activity.status);
                  const actor = agentsByThread.get(activity.thread_id);
                  const timestamp = activityTime(activity.created_at);
                  const actorName = actor ? agentLabel(actor) : "Runtime Agent";
                  const action = activityAction(activity);
                  const activityHeadingId = `web-supervisor-activity-${activity.sequence}-${
                    activity.item_id ?? activity.kind
                  }`;
                  const activityActorId = `${activityHeadingId}-actor`;
                  const activityStatusId = `${activityHeadingId}-status`;
                  return (
                    <li
                      key={activityIdentity(activity)}
                      className={`is-${status.tone}`}
                    >
                      <span
                        className={`is-${status.tone} is-${activity.status}`}
                        aria-hidden="true"
                      />
                      <div className="web-supervisor-activity-copy">
                        <article
                          className="web-supervisor-activity-card"
                          role={status.tone === "error"
                            ? "alert"
                            : status.tone === "waiting" ? "status" : undefined}
                          aria-labelledby={`${activityActorId} ${activityHeadingId}`}
                          aria-describedby={activityStatusId}
                        >
                          <div className="web-supervisor-activity-meta">
                            <span
                              id={activityActorId}
                              className="web-supervisor-activity-actor"
                            >
                              {actorName}
                            </span>
                            <span className="web-supervisor-activity-arrow" aria-hidden="true">→</span>
                            <h3 id={activityHeadingId} className="web-supervisor-activity-action">
                              {action}
                            </h3>
                            <span
                              id={activityStatusId}
                              className={`web-supervisor-activity-status is-${status.tone}`}
                              aria-label={`Status: ${status.label}`}
                            >
                              {status.label}
                            </span>
                            {timestamp ? (
                              <time dateTime={activity.created_at}>{timestamp}</time>
                            ) : null}
                          </div>
                          {activity.detail ? (
                            <details className="web-supervisor-activity-detail">
                              <summary
                                role="button"
                                aria-label="Show safe activity detail"
                              >
                                Show details
                              </summary>
                              <p>{activity.detail}</p>
                            </details>
                          ) : null}
                        </article>
                      </div>
                    </li>
                  );
                })}
              </ol>
            ) : (
              <p className="web-supervisor-overview-empty">
                No persisted Agent behavior has been observed yet.
              </p>
            )}
          </div>

          <div className="web-supervisor-artifacts" aria-label="Evidence Artifacts">
            <div className="web-supervisor-section-heading">
              <FileCheck2 size={14} aria-hidden="true" />
              <strong>Evidence Artifacts</strong>
              <span>{artifacts.length}</span>
            </div>
            {artifacts.length ? (
              <div className="web-supervisor-artifact-list" role="list">
                {artifacts.map((artifact) => (
                  <div className="web-supervisor-artifact" role="listitem" key={artifact.id}>
                    <span className="web-supervisor-artifact-copy">
                      <strong>{artifact.artifact_schema}</strong>
                      <span>
                        {artifact.producer_agent_role ?? "Root Supervisor"}
                        {artifact.byte_size !== null
                          ? ` · ${artifact.byte_size.toLocaleString()} bytes`
                          : ""}
                      </span>
                    </span>
                    <span className={`web-supervisor-artifact-state is-${artifact.state}`}>
                      {artifact.state}
                    </span>
                    {onLoadArtifactContent && artifact.state === "ready" ? (
                      <button
                        type="button"
                        className="web-supervisor-review-action is-compact"
                        onClick={() => void openArtifactContent(artifact)}
                      >
                        Open
                      </button>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : (
              <p className="web-supervisor-overview-empty">
                Waiting for validated Runtime Resources.
              </p>
            )}
          </div>
        </>
      )}
      </section>
      {review ? (
        <ModalShell
          className="web-supervisor-review-modal"
          cardClassName="web-supervisor-review-card"
          ariaLabelledBy="web-supervisor-review-title"
          onBackdropClick={() => setReview(null)}
        >
          <header>
            <div>
              <span>{review.kind === "agent" ? "Runtime Thread" : "Authorized Artifact"}</span>
              <h2 id="web-supervisor-review-title">{review.title}</h2>
            </div>
            <button
              type="button"
              onClick={() => setReview(null)}
              aria-label="Close review"
            >
              <X size={16} aria-hidden="true" />
            </button>
          </header>
          <div className="web-supervisor-review-body">
            {review.loading ? (
              <p className="web-supervisor-review-loading" role="status">
                <LoaderCircle size={15} aria-hidden="true" />
                Loading authoritative content...
              </p>
            ) : review.error ? (
              <p className="web-supervisor-overview-error" role="alert">{review.error}</p>
            ) : review.kind === "agent" ? (
              review.turns.length ? (
                <ol className="web-supervisor-review-turns">
                  {review.turns.map((turn, index) => (
                    <li key={turn.id}>
                      <div>
                        <strong>Turn {index + 1}</strong>
                        <span>{turn.status}</span>
                      </div>
                      {turn.items.length ? (
                        <ol>
                          {turn.items.map((item, itemIndex) => (
                            <li key={`${turn.id}-${String(item.id ?? itemIndex)}`}>
                              <span>{typeof item.type === "string" ? item.type : "Runtime item"}</span>
                              <p>{reviewItemText(item)}</p>
                            </li>
                          ))}
                        </ol>
                      ) : (
                        <p>No browser-safe items were recorded for this Turn.</p>
                      )}
                    </li>
                  ))}
                </ol>
              ) : (
                <p className="web-supervisor-overview-empty">
                  This Agent has no recorded Turns.
                </p>
              )
            ) : (
              <pre>{JSON.stringify(review.content, null, 2)}</pre>
            )}
          </div>
        </ModalShell>
      ) : null}
    </>
  );
}
