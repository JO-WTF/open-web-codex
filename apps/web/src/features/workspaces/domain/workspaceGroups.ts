import type { WorkspaceInfo } from "../../../types";

export const RESERVED_GROUP_NAME = "Projects";

export type WorkspaceGroupSection = {
  id: null;
  name: string;
  workspaces: WorkspaceInfo[];
};

export function buildGroupedWorkspaces(
  workspaces: WorkspaceInfo[],
): WorkspaceGroupSection[] {
  const roots = workspaces
    .filter((entry) => (entry.kind ?? "main") !== "worktree" && !entry.parentId)
    .slice()
    .sort((left, right) => left.name.localeCompare(right.name));
  return roots.length
    ? [{ id: null, name: RESERVED_GROUP_NAME, workspaces: roots }]
    : [];
}
