import { useCallback, useState } from "react";
import type { WorkspaceInfo } from "../../../types";

type WorktreePromptState = {
  workspace: WorkspaceInfo;
  name: string;
  branch: string;
  branchWasEdited: boolean;
  copyAgentsMd: boolean;
  isSubmitting: boolean;
  error: string | null;
} | null;

type UseWorktreePromptOptions = {
  addWorktreeAgent: (
    workspace: WorkspaceInfo,
    branch: string,
    options?: { displayName?: string | null; copyAgentsMd?: boolean },
  ) => Promise<WorkspaceInfo | null>;
  connectWorkspace: (workspace: WorkspaceInfo) => Promise<void>;
  onSelectWorkspace: (workspaceId: string) => void;
  onCompactActivate?: () => void;
  onError?: (message: string) => void;
};

type UseWorktreePromptResult = {
  worktreePrompt: WorktreePromptState;
  openPrompt: (workspace: WorkspaceInfo) => void;
  confirmPrompt: () => Promise<void>;
  cancelPrompt: () => void;
  updateName: (value: string) => void;
  updateBranch: (value: string) => void;
  updateCopyAgentsMd: (value: boolean) => void;
};

function toBranchFromName(value: string): string | null {
  const trimmed = value.trim().toLowerCase();
  if (!trimmed) {
    return null;
  }
  const slug = trimmed
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/(^-|-$)/g, "");
  if (!slug) {
    return null;
  }
  return `codex/${slug}`;
}

export function useWorktreePrompt({
  addWorktreeAgent,
  connectWorkspace,
  onSelectWorkspace,
  onCompactActivate,
  onError,
}: UseWorktreePromptOptions): UseWorktreePromptResult {
  const [worktreePrompt, setWorktreePrompt] = useState<WorktreePromptState>(null);

  const openPrompt = useCallback((workspace: WorkspaceInfo) => {
    const defaultBranch = `codex/${new Date().toISOString().slice(0, 10)}-${Math.random()
      .toString(36)
      .slice(2, 6)}`;
    setWorktreePrompt({
      workspace,
      name: "",
      branch: defaultBranch,
      branchWasEdited: false,
      copyAgentsMd: true,
      isSubmitting: false,
      error: null,
    });
  }, []);

  const updateName = useCallback((value: string) => {
    setWorktreePrompt((prev) => {
      if (!prev) {
        return prev;
      }
      if (prev.branchWasEdited) {
        return { ...prev, name: value, error: null };
      }
      const nextBranch = toBranchFromName(value);
      if (!nextBranch) {
        return { ...prev, name: value, error: null };
      }
      return {
        ...prev,
        name: value,
        branch: nextBranch,
        error: null,
      };
    });
  }, []);

  const updateBranch = useCallback((value: string) => {
    setWorktreePrompt((prev) =>
      prev ? { ...prev, branch: value, branchWasEdited: true, error: null } : prev,
    );
  }, []);

  const updateCopyAgentsMd = useCallback((value: boolean) => {
    setWorktreePrompt((prev) => (prev ? { ...prev, copyAgentsMd: value } : prev));
  }, []);

  const cancelPrompt = useCallback(() => {
    setWorktreePrompt(null);
  }, []);

  const confirmPrompt = useCallback(async () => {
    if (!worktreePrompt || worktreePrompt.isSubmitting) {
      return;
    }
    const snapshot = worktreePrompt;
    setWorktreePrompt((prev) =>
      prev ? { ...prev, isSubmitting: true, error: null } : prev,
    );

    try {
      const displayName = snapshot.name.trim();
      const worktreeWorkspace = await addWorktreeAgent(snapshot.workspace, snapshot.branch, {
        displayName: displayName.length > 0 ? displayName : null,
        copyAgentsMd: snapshot.copyAgentsMd,
      });
      if (!worktreeWorkspace) {
        setWorktreePrompt(null);
        return;
      }
      onSelectWorkspace(worktreeWorkspace.id);
      if (!worktreeWorkspace.connected) {
        await connectWorkspace(worktreeWorkspace);
      }
      onCompactActivate?.();
      setWorktreePrompt(null);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setWorktreePrompt((prev) =>
        prev ? { ...prev, isSubmitting: false, error: message } : prev,
      );
      onError?.(message);
    }
  }, [
    addWorktreeAgent,
    connectWorkspace,
    onCompactActivate,
    onError,
    onSelectWorkspace,
    worktreePrompt,
  ]);

  return {
    worktreePrompt,
    openPrompt,
    confirmPrompt,
    cancelPrompt,
    updateName,
    updateBranch,
    updateCopyAgentsMd,
  };
}
