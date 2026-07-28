import Bot from "lucide-react/dist/esm/icons/bot";
import FileCheck2 from "lucide-react/dist/esm/icons/file-check-2";
import History from "lucide-react/dist/esm/icons/history";
import Network from "lucide-react/dist/esm/icons/network";
import ShieldCheck from "lucide-react/dist/esm/icons/shield-check";
import type {
  ArtifactSummary,
  RuntimeAgentActivity,
  RuntimeAgentExecution,
  RuntimeAgentProjection,
  SupervisorPolicyBinding,
} from "../../../browser/types";
import TaskApprovalQueue, {
  type TaskApprovalRequest,
} from "./TaskApprovalQueue";

type Props = {
  taskTitle: string;
  policy: SupervisorPolicyBinding | null;
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
  loading?: boolean;
  error?: string | null;
};

type StatusTone = "idle" | "active" | "waiting" | "terminal" | "error";

type AgentExecutionStatus = RuntimeAgentExecution["status"];
type AgentActivityStatus = RuntimeAgentActivity["status"];

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
    case "completed":
      return { label: "Completed", tone: "terminal" };
    case "failed":
      return { label: "Failed", tone: "error" };
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
    case "completed":
      return "Completion";
    case "failed":
      return "Failure";
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

export default function SupervisorOverview({
  taskTitle,
  policy,
  agents,
  activities = [],
  executions = [],
  artifacts,
  approvals = [],
  onResolveApproval,
  loading = false,
  error = null,
}: Props) {
  const rootAgent = agents.find((agent) => agent.is_root) ?? null;
  const rootActivities = activities
    .filter((activity) => rootAgent && activity.thread_id === rootAgent.thread_id)
    .sort((left, right) => left.sequence - right.sequence);
  const rootBehavior = [...rootActivities]
    .reverse()
    .find((activity) =>
      activity.kind !== "assignment"
      && activity.kind !== "reporting");
  const rootProgress = [...rootActivities]
    .reverse()
    .find((activity) => activity.kind === "reporting");
  const agentsByThread = new Map(agents.map((agent) => [agent.thread_id, agent]));
  const timelineActivities = [...activities].sort((left, right) =>
    left.sequence - right.sequence);
  const rootStatus = rootAgent
    ? agentStatusPresentation(rootAgent)
    : { label: "Starting", tone: "idle" as const };

  return (
    <section className="web-supervisor-overview" aria-label="Enterprise Supervisor collaboration">
      <div className="web-supervisor-overview-heading">
        <span className="web-supervisor-overview-icon" aria-hidden="true">
          <ShieldCheck size={16} />
        </span>
        <div>
          <strong>{policy?.display_name ?? "Agent collaboration"}</strong>
          <span>
            {policy
              ? `Policy ${policy.policy_id} · ${policy.version}`
              : "Runtime-owned Agent collaboration"}
          </span>
        </div>
        {policy ? (
          <span className={`web-supervisor-binding is-${policy.state}`}>
            {policy.state}
          </span>
        ) : null}
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
                  {rootBehavior?.detail ?? rootBehavior?.title ?? "Coordinating the collaboration"}
                </Detail>
                <Detail label="Latest progress">
                  {rootProgress?.detail ?? rootProgress?.title ?? "No progress reported yet"}
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
                            <strong>{agent ? agentLabel(agent) : "Runtime Agent"}</strong>
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
                        </dl>
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
                {timelineActivities.map((activity, index) => {
                  const status = activityStatusPresentation(activity.status);
                  const actor = agentsByThread.get(activity.thread_id);
                  const timestamp = activityTime(activity.created_at);
                  return (
                    <li
                      key={`${activity.sequence}-${activity.thread_id}-${activity.item_id ?? activity.kind}-${index}`}
                    >
                      <span className={`is-${activity.status}`} aria-hidden="true" />
                      <div className="web-supervisor-activity-copy">
                        <div className="web-supervisor-activity-meta">
                          <span>{actor ? agentLabel(actor) : "Runtime Agent"}</span>
                          <span>{activityKindLabel(activity.kind)}</span>
                          <span>{status.label}</span>
                          {timestamp ? (
                            <time dateTime={activity.created_at}>{timestamp}</time>
                          ) : null}
                        </div>
                        <strong>{activity.title}</strong>
                        {activity.detail ? <p>{activity.detail}</p> : null}
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
  );
}
