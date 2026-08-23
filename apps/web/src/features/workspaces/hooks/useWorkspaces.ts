import { useEffect, useMemo, useState } from "react";
import type { DebugEntry, WorkspaceInfo } from "../../../types";
import { buildGroupedWorkspaces } from "../domain/workspaceGroups";
import {
  useWorkspaceCrud,
  type AddWorkspacesFromPathsResult,
} from "./useWorkspaceCrud";
import { useWorktreeOps } from "./useWorktreeOps";

export type UseWorkspacesOptions = {
  onDebug?: (entry: DebugEntry) => void;
};

export type UseWorkspacesResult = {
  workspaces: WorkspaceInfo[];
  groupedWorkspaces: ReturnType<typeof buildGroupedWorkspaces>;
  activeWorkspace: WorkspaceInfo | null;
  activeWorkspaceId: string | null;
  setActiveWorkspaceId: (workspaceId: string | null) => void;
  addWorkspaceFromPath: (path: string, options?: { activate?: boolean }) => Promise<WorkspaceInfo | null>;
  addWorkspaceFromGitUrl: (
    url: string,
    destinationPath: string,
    targetFolderName?: string | null,
    options?: { activate?: boolean },
  ) => Promise<WorkspaceInfo | null>;
  addWorkspacesFromPaths: (paths: string[]) => Promise<AddWorkspacesFromPathsResult>;
  filterWorkspacePaths: (paths: string[]) => Promise<string[]>;
  addCloneAgent: (source: WorkspaceInfo, copyName: string, copiesFolder: string) => Promise<WorkspaceInfo | null>;
  addWorktreeAgent: (
    parent: WorkspaceInfo,
    branch: string,
    options?: {
      activate?: boolean;
      displayName?: string | null;
      copyAgentsMd?: boolean;
    },
  ) => Promise<WorkspaceInfo | null>;
  connectWorkspace: (entry: WorkspaceInfo) => Promise<void>;
  markWorkspaceConnected: (id: string) => void;
  removeWorkspace: (workspaceId: string) => Promise<void>;
  removeWorktree: (workspaceId: string) => Promise<void>;
  renameWorktree: (workspaceId: string, branch: string) => Promise<WorkspaceInfo>;
  renameWorktreeUpstream: (workspaceId: string, oldBranch: string, newBranch: string) => Promise<void>;
  deletingWorktreeIds: Set<string>;
  hasLoaded: boolean;
  refreshWorkspaces: () => Promise<WorkspaceInfo[] | undefined>;
};

export function useWorkspaces(options: UseWorkspacesOptions = {}): UseWorkspacesResult {
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([]);
  const [activeWorkspaceId, setActiveWorkspaceId] = useState<string | null>(null);
  const [hasLoaded, setHasLoaded] = useState(false);
  const { onDebug } = options;

  const {
    addWorkspaceFromPath,
    addWorkspaceFromGitUrl,
    addWorkspacesFromPaths,
    connectWorkspace,
    filterWorkspacePaths,
    markWorkspaceConnected,
    refreshWorkspaces,
    removeWorkspace,
  } = useWorkspaceCrud({
    onDebug,
    workspaces,
    setWorkspaces,
    setActiveWorkspaceId,
    setHasLoaded,
  });

  useEffect(() => {
    void refreshWorkspaces();
  }, [refreshWorkspaces]);

  const activeWorkspace = useMemo(
    () => workspaces.find((entry) => entry.id === activeWorkspaceId) ?? null,
    [activeWorkspaceId, workspaces],
  );

  const groupedWorkspaces = useMemo(
    () => buildGroupedWorkspaces(workspaces),
    [workspaces],
  );

  const {
    addCloneAgent,
    addWorktreeAgent,
    deletingWorktreeIds,
    removeWorktree,
    renameWorktree,
    renameWorktreeUpstream,
  } = useWorktreeOps({
    onDebug,
    setWorkspaces,
    setActiveWorkspaceId,
  });

  return {
    workspaces,
    groupedWorkspaces,
    activeWorkspace,
    activeWorkspaceId,
    setActiveWorkspaceId,
    addWorkspaceFromPath,
    addWorkspaceFromGitUrl,
    addWorkspacesFromPaths,
    filterWorkspacePaths,
    addCloneAgent,
    addWorktreeAgent,
    connectWorkspace,
    markWorkspaceConnected,
    removeWorkspace,
    removeWorktree,
    renameWorktree,
    renameWorktreeUpstream,
    deletingWorktreeIds,
    hasLoaded,
    refreshWorkspaces,
  };
}
