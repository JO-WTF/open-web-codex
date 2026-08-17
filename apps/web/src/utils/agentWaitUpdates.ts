import type {
  RuntimeAgentActivity,
  RuntimeAgentProjection,
} from "../../browser/types";

export type AgentWaitHistoryUpdate = {
  threadId: string;
  agentLabel: string;
  text: string;
  status: string;
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
    // `starting` is a provisional Thread projection. A later durable Item
    // can already have reached its terminal state before that projection is
    // replaced, so it must not hide the newest History status in the card.
    const status = waiting
      ? "waiting"
      : reportedStatus?.toLowerCase() === "starting" && latest
        ? latest.status
        : reportedStatus || latest?.status || "pending";
    return {
      threadId: agent.thread_id,
      agentLabel: agentLabel(agent),
      text: latest?.title.trim() || "No recorded History item yet.",
      status,
    };
  });
}
