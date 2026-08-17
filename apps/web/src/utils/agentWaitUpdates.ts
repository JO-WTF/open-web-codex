import type {
  RuntimeAgentActivity,
  RuntimeAgentProjection,
} from "../../browser/types";

export type AgentWaitHistoryUpdate = {
  threadId: string;
  agentLabel: string;
  text: string;
  status: string;
  kind: RuntimeAgentActivity["kind"] | null;
};

function agentLabel(agent: RuntimeAgentProjection): string {
  return agent.agent_nickname?.trim()
    || agent.agent_role?.trim()
    || "Runtime Agent";
}

/**
 * Map each child Agent to the newest persisted Runtime Item projected for its
 * Thread. Turn lifecycle rows are intentionally excluded: the waiting card
 * should mirror the latest History item, not replace it with a generic
 * "Turn completed" status emitted afterwards.
 */
export function latestAgentHistoryUpdates(
  agents: RuntimeAgentProjection[],
  activities: RuntimeAgentActivity[],
): AgentWaitHistoryUpdate[] {
  const childAgents = agents.filter((agent) => !agent.is_root);
  const childThreadIds = new Set(childAgents.map((agent) => agent.thread_id));
  const latestByThread = new Map<string, RuntimeAgentActivity>();

  for (const activity of activities) {
    if (!activity.item_id || !childThreadIds.has(activity.thread_id)) continue;
    const previous = latestByThread.get(activity.thread_id);
    if (!previous || activity.sequence > previous.sequence) {
      latestByThread.set(activity.thread_id, activity);
    }
  }

  return childAgents.map((agent) => {
    const latest = latestByThread.get(agent.thread_id);
    const waiting = agent.active_flags.some((flag) => flag.toLowerCase().includes("waiting"));
    const reportedStatus = agent.status_type?.trim();
    // This row describes `latest`, not the Agent as a whole. Runtime Agent
    // status remains active while an Item can already be terminal, so using
    // it first created contradictory cards such as “Starting / Completed
    // codex · list mcp resources”. Only fall back to the Agent projection
    // before any durable Item has been observed.
    const status = latest?.status
      || (waiting ? "waiting" : reportedStatus || "pending");
    return {
      threadId: agent.thread_id,
      agentLabel: agentLabel(agent),
      // Reasoning may arrive with incremental text while its Item is still
      // running. Keep the wait card stable until the terminal projection for
      // that same Item supplies its bounded final summary.
      text: latest?.kind === "reasoning" && latest.status === "running"
        ? "Thinking"
        : latest?.title.trim() || "No recorded History item yet.",
      status,
      kind: latest?.kind ?? null,
    };
  });
}
