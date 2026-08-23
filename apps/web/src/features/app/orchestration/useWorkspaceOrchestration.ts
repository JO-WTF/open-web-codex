import { useEffect, useMemo, useState } from "react";
import type { WorkspaceInfo } from "@/types";
import { useLocalUsage } from "@/features/home/hooks/useLocalUsage";

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
  workspacesById: Map<string, WorkspaceInfo>;
  hasLoaded: boolean;
  showHome: boolean;
  threadsByWorkspace: Record<string, ThreadSummary[]>;
  lastAgentMessageByThread: Record<string, LastAgentMessage | undefined>;
  threadStatusById: Record<string, ThreadStatus | undefined>;
  threadListLoadingByWorkspace: Record<string, boolean | undefined>;
};

export function useWorkspaceInsightsOrchestration({
  workspaces,
  workspacesById,
  hasLoaded,
  showHome,
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

  const [usageMetric, setUsageMetric] = useState<"tokens" | "time">("tokens");
  const [usageWorkspaceId, setUsageWorkspaceId] = useState<string | null>(null);

  const usageWorkspaceOptions = useMemo(
    () =>
      workspaces.map((workspace) => ({ id: workspace.id, label: workspace.name })),
    [workspaces],
  );

  const usageWorkspacePath = useMemo(() => {
    if (!usageWorkspaceId) {
      return null;
    }
    return workspacesById.get(usageWorkspaceId)?.path ?? null;
  }, [usageWorkspaceId, workspacesById]);

  useEffect(() => {
    if (!usageWorkspaceId) {
      return;
    }
    if (workspaces.some((workspace) => workspace.id === usageWorkspaceId)) {
      return;
    }
    setUsageWorkspaceId(null);
  }, [usageWorkspaceId, workspaces]);

  const {
    snapshot: localUsageSnapshot,
    isLoading: isLoadingLocalUsage,
    error: localUsageError,
    refresh: refreshLocalUsage,
  } = useLocalUsage(showHome, usageWorkspacePath);

  return {
    latestAgentRuns,
    isLoadingLatestAgents,
    usageMetric,
    setUsageMetric,
    usageWorkspaceId,
    setUsageWorkspaceId,
    usageWorkspaceOptions,
    localUsageSnapshot,
    isLoadingLocalUsage,
    localUsageError,
    refreshLocalUsage,
  };
}
