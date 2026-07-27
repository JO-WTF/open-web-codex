import Bot from "lucide-react/dist/esm/icons/bot";
import FileCheck2 from "lucide-react/dist/esm/icons/file-check-2";
import Network from "lucide-react/dist/esm/icons/network";
import ShieldCheck from "lucide-react/dist/esm/icons/shield-check";
import type {
  ArtifactSummary,
  RuntimeAgentActivity,
  RuntimeAgentProjection,
  SupervisorPolicyBinding,
} from "../../../browser/types";

type Props = {
  taskTitle: string;
  policy: SupervisorPolicyBinding | null;
  agents: RuntimeAgentProjection[];
  activities?: RuntimeAgentActivity[];
  artifacts: ArtifactSummary[];
  loading?: boolean;
  error?: string | null;
};

type StatusTone = "idle" | "active" | "waiting" | "terminal" | "error";

type AgentExecutionStatus =
  | "pending"
  | "running"
  | "waiting"
  | "completed"
  | "failed"
  | "interrupted";

type AgentExecutionNode = {
  id: string;
  agent: RuntimeAgentProjection;
  ordinal: number;
  task: string;
  status: AgentExecutionStatus;
  turnId: string | null;
  currentBehavior: string;
  latestProgress: string | null;
  assignmentSequence: number;
  hasObservedTask: boolean;
};

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

function activityTurnKey(threadId: string, turnId: string): string {
  return `${threadId}\u0000${turnId}`;
}

/**
 * Build presentation-only task executions from the safe event projection.
 *
 * A spawn assignment creates the first pending node. Later instructions are
 * promoted to a new node only when the receiver actually starts another
 * Runtime Turn; a queued message by itself never invents an execution. Only
 * activities from the bound Turn can update that node, so reusing the same
 * Agent Thread cannot rewrite a completed task.
 */
export function buildAgentExecutionNodes(
  agents: RuntimeAgentProjection[],
  activities: RuntimeAgentActivity[],
): AgentExecutionNode[] {
  const childAgents = new Map(
    agents.filter((agent) => !agent.is_root).map((agent) => [agent.thread_id, agent]),
  );
  const nodes: AgentExecutionNode[] = [];
  const nodesByThread = new Map<string, AgentExecutionNode[]>();
  const nodesByTurn = new Map<string, AgentExecutionNode>();
  const pendingInstructions = new Map<string, RuntimeAgentActivity[]>();
  const ordinals = new Map<string, number>();

  const currentNode = (threadId: string) => {
    const threadNodes = nodesByThread.get(threadId) ?? [];
    return [...threadNodes].reverse().find((node) =>
      node.status === "running" || node.status === "waiting")
      ?? threadNodes[threadNodes.length - 1];
  };

  const createNode = (
    agent: RuntimeAgentProjection,
    input: {
      task: string;
      status: AgentExecutionStatus;
      turnId: string | null;
      currentBehavior: string;
      sourceSequence: number;
      hasObservedTask: boolean;
    },
  ) => {
    const ordinal = (ordinals.get(agent.thread_id) ?? 0) + 1;
    ordinals.set(agent.thread_id, ordinal);
    const node: AgentExecutionNode = {
      id: `${agent.run_id}:${agent.thread_id}:${input.sourceSequence}`,
      agent,
      ordinal,
      task: input.task,
      status: input.status,
      turnId: input.turnId,
      currentBehavior: input.currentBehavior,
      latestProgress: null,
      assignmentSequence: input.sourceSequence,
      hasObservedTask: input.hasObservedTask,
    };
    nodes.push(node);
    const threadNodes = nodesByThread.get(agent.thread_id) ?? [];
    threadNodes.push(node);
    nodesByThread.set(agent.thread_id, threadNodes);
    if (input.turnId) {
      nodesByTurn.set(activityTurnKey(agent.thread_id, input.turnId), node);
    }
    return node;
  };

  for (const activity of [...activities].sort((left, right) => left.sequence - right.sequence)) {
    const agent = childAgents.get(activity.thread_id);
    if (!agent) continue;

    if (activity.kind === "assignment") {
      const unmatchedTurn = [...(nodesByThread.get(activity.thread_id) ?? [])]
        .reverse()
        .find((candidate) => !candidate.hasObservedTask && candidate.turnId !== null);
      if (unmatchedTurn) {
        unmatchedTurn.task = activity.detail ?? "Assigned task";
        unmatchedTurn.hasObservedTask = true;
        continue;
      }
      createNode(agent, {
        task: activity.detail ?? "Assigned task",
        status: "pending",
        turnId: null,
        currentBehavior: activity.title,
        sourceSequence: activity.sequence,
        hasObservedTask: true,
      });
      continue;
    }

    if (activity.kind === "guidance") {
      const instructions = pendingInstructions.get(activity.thread_id) ?? [];
      instructions.push(activity);
      pendingInstructions.set(activity.thread_id, instructions);
      continue;
    }

    let node: AgentExecutionNode | undefined;
    if (activity.kind === "turn_started" && activity.turn_id) {
      const key = activityTurnKey(activity.thread_id, activity.turn_id);
      node = nodesByTurn.get(key);
      if (!node) {
        node = (nodesByThread.get(activity.thread_id) ?? [])
          .find((candidate) => candidate.turnId === null);
        if (node) {
          node.turnId = activity.turn_id;
          nodesByTurn.set(key, node);
        } else {
          const instructions = pendingInstructions.get(activity.thread_id) ?? [];
          const instruction = instructions[instructions.length - 1];
          node = createNode(agent, {
            task: instruction?.detail ?? "Runtime task",
            status: "running",
            turnId: activity.turn_id,
            currentBehavior: activity.title,
            sourceSequence: instruction?.sequence ?? activity.sequence,
            hasObservedTask: Boolean(instruction),
          });
        }
      }
      pendingInstructions.delete(activity.thread_id);
    } else if (activity.turn_id) {
      node = nodesByTurn.get(activityTurnKey(activity.thread_id, activity.turn_id));
    } else {
      node = currentNode(activity.thread_id);
    }
    if (!node) continue;

    if (activity.kind === "reporting") {
      node.latestProgress = activity.detail ?? activity.title;
    } else {
      node.currentBehavior = activity.detail ?? activity.title;
    }

    switch (activity.kind) {
      case "turn_started":
        node.status = "running";
        break;
      case "waiting":
        node.status = "waiting";
        break;
      case "turn_completed":
      case "completed":
        node.status = "completed";
        break;
      case "failed":
        node.status = "failed";
        break;
      case "interrupted":
        node.status = "interrupted";
        break;
      default:
        break;
    }
  }

  return nodes.sort((left, right) => left.assignmentSequence - right.assignmentSequence);
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
  artifacts,
  loading = false,
  error = null,
}: Props) {
  if (!policy && agents.length === 0 && !loading && !error) return null;

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
  const executions = buildAgentExecutionNodes(agents, activities);
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
                  return (
                    <li key={execution.id}>
                      <span className={`web-supervisor-stream-node is-${status.tone}`} aria-hidden="true" />
                      <article className={`web-supervisor-agent is-execution is-${execution.status}`}>
                        <div className="web-supervisor-agent-summary">
                          <span className="web-supervisor-agent-icon" aria-hidden="true">
                            <Network size={15} />
                          </span>
                          <span className="web-supervisor-agent-copy">
                            <strong>{agentLabel(execution.agent)}</strong>
                            <span>
                              {execution.agent.agent_role ?? "Runtime Agent"}
                              {" · "}
                              Task {execution.ordinal}
                            </span>
                          </span>
                          <StatusBadge {...status} />
                        </div>
                        <dl className="web-supervisor-agent-details">
                          <Detail label="Current task">{execution.task}</Detail>
                          <Detail label="Task status">{status.label}</Detail>
                          <Detail label="Current behavior">{execution.currentBehavior}</Detail>
                          <Detail label="Latest progress">
                            {execution.latestProgress ?? "No progress reported yet"}
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
