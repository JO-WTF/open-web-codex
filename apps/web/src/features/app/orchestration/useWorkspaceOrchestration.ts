import { useMemo } from "react";
import type { WorkspaceInfo } from "@/types";

type ThreadSummary = {
  id: string;
  name?: string | null;
  updatedAt: number;
};

type LastAgentMessage = {
  text: string;
  timestamp: number;
};

type ThreadStatus = {
  isProcessing?: boolean;
};

type UseWorkspaceInsightsOrchestrationOptions = {
  workspaces: WorkspaceInfo[];
  hasLoaded: boolean;
  threadsByWorkspace: Record<string, ThreadSummary[]>;
  lastAgentMessageByThread: Record<string, LastAgentMessage | undefined>;
  threadStatusById: Record<string, ThreadStatus | undefined>;
  threadListLoadingByWorkspace: Record<string, boolean | undefined>;
};

export function useWorkspaceInsightsOrchestration({
  workspaces,
  hasLoaded,
  threadsByWorkspace,
  lastAgentMessageByThread,
  threadStatusById,
  threadListLoadingByWorkspace,
}: UseWorkspaceInsightsOrchestrationOptions) {
  const latestAgentRuns = useMemo(() => {
    const entries: Array<{
      threadId: string;
      message: string;
      timestamp: number;
      projectName: string;
      workspaceId: string;
      isProcessing: boolean;
    }> = [];

    workspaces.forEach((workspace) => {
      const threads = threadsByWorkspace[workspace.id] ?? [];
      threads.forEach((thread) => {
        const entry = lastAgentMessageByThread[thread.id];
        if (!entry) {
          return;
        }
        entries.push({
          threadId: thread.id,
          message: entry.text,
          timestamp: entry.timestamp,
          projectName: workspace.name,
          workspaceId: workspace.id,
          isProcessing: threadStatusById[thread.id]?.isProcessing ?? false,
        });
      });
    });

    return entries.sort((a, b) => b.timestamp - a.timestamp).slice(0, 3);
  }, [
    lastAgentMessageByThread,
    threadStatusById,
    threadsByWorkspace,
    workspaces,
  ]);

  const isLoadingLatestAgents = useMemo(
    () =>
      !hasLoaded || workspaces.some((workspace) => threadListLoadingByWorkspace[workspace.id] ?? false),
    [hasLoaded, threadListLoadingByWorkspace, workspaces],
  );

  return {
    latestAgentRuns,
    isLoadingLatestAgents,
  };
}
