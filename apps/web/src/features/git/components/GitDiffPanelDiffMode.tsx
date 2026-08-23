import type { MouseEvent as ReactMouseEvent } from "react";
import Download from "lucide-react/dist/esm/icons/download";
import RotateCcw from "lucide-react/dist/esm/icons/rotate-ccw";
import Upload from "lucide-react/dist/esm/icons/upload";
import { CommitButton, DiffSection, type DiffFile } from "./GitDiffPanelShared";
import { isGitRootNotFound, isMissingRepo } from "./GitDiffPanel.utils";

type GitDiffModeContentProps = {
  error: string | null | undefined;
  onInitGitRepo?: () => void | Promise<void>;
  initGitRepoLoading: boolean;
  showCommitMessage: boolean;
  showApplyWorktree: boolean;
  commitMessage: string;
  onCommitMessageChange?: (value: string) => void;
  worktreeApplyTitle: string | null;
  worktreeApplyLoading: boolean;
  worktreeApplySuccess: boolean;
  onApplyWorktreeChanges?: () => void | Promise<void>;
  stagedFiles: DiffFile[];
  unstagedFiles: DiffFile[];
  commitLoading: boolean;
  onCommit?: () => void | Promise<void>;
  commitsAhead: number;
  commitsBehind: number;
  onPull?: () => void | Promise<void>;
  pullLoading: boolean;
  onPush?: () => void | Promise<void>;
  pushLoading: boolean;
  onSync?: () => void | Promise<void>;
  syncLoading: boolean;
  onStageAllChanges?: () => void | Promise<void>;
  onStageFile?: (path: string) => Promise<void> | void;
  onUnstageFile?: (path: string) => Promise<void> | void;
  onDiscardFile?: (path: string) => Promise<void> | void;
  onDiscardFiles?: (paths: string[]) => Promise<void> | void;
  onReviewUncommittedChanges?: () => void | Promise<void>;
  selectedFiles: Set<string>;
  selectedPath: string | null;
  onSelectFile?: (path: string) => void;
  onFileClick: (
    event: ReactMouseEvent<HTMLDivElement>,
    path: string,
    section: "staged" | "unstaged",
  ) => void;
  onShowFileMenu: (
    event: ReactMouseEvent<HTMLDivElement>,
    path: string,
    section: "staged" | "unstaged",
  ) => void;
  onDiffListClick: (event: ReactMouseEvent<HTMLDivElement>) => void;
};

export function GitDiffModeContent({
  error,
  onInitGitRepo,
  initGitRepoLoading,
  showCommitMessage,
  showApplyWorktree,
  commitMessage,
  onCommitMessageChange,
  worktreeApplyTitle,
  worktreeApplyLoading,
  worktreeApplySuccess,
  onApplyWorktreeChanges,
  stagedFiles,
  unstagedFiles,
  commitLoading,
  onCommit,
  commitsAhead,
  commitsBehind,
  onPull,
  pullLoading,
  onPush,
  pushLoading,
  onSync,
  syncLoading,
  onStageAllChanges,
  onStageFile,
  onUnstageFile,
  onDiscardFile,
  onDiscardFiles,
  onReviewUncommittedChanges,
  selectedFiles,
  selectedPath,
  onSelectFile,
  onFileClick,
  onShowFileMenu,
  onDiffListClick,
}: GitDiffModeContentProps) {
  const missingRepo = isMissingRepo(error);
  const showInitGitRepo =
    Boolean(onInitGitRepo) && missingRepo && !isGitRootNotFound(error);
  const showWorktreeApplyInUnstaged = showApplyWorktree && unstagedFiles.length > 0;
  const showWorktreeApplyInStaged =
    showApplyWorktree && unstagedFiles.length === 0 && stagedFiles.length > 0;

  return (
    <div className="diff-list" onClick={onDiffListClick}>
      {showInitGitRepo && (
        <div className="git-root-panel">
          <div className="git-root-title">This workspace isn't a Git repository yet.</div>
          <div className="git-root-primary-action">
            <button
              type="button"
              className="primary git-root-button"
              onClick={() => {
                void onInitGitRepo?.();
              }}
              disabled={initGitRepoLoading}
            >
              {initGitRepoLoading ? "Initializing..." : "Initialize Git"}
            </button>
          </div>
        </div>
      )}
      {showCommitMessage && (
        <div className="commit-message-section">
          <div className="commit-message-input-wrapper">
            <textarea
              className="commit-message-input"
              placeholder="Commit message..."
              value={commitMessage}
              onChange={(event) => onCommitMessageChange?.(event.target.value)}
              rows={2}
            />
          </div>
          <CommitButton
            commitMessage={commitMessage}
            hasStagedFiles={stagedFiles.length > 0}
            hasUnstagedFiles={unstagedFiles.length > 0}
            commitLoading={commitLoading}
            onCommit={onCommit}
          />
        </div>
      )}
      {(commitsAhead > 0 || commitsBehind > 0) && !stagedFiles.length && (
        <div className="push-section">
          <div className="push-sync-buttons">
            {commitsBehind > 0 && (
              <button
                type="button"
                className="push-button-secondary"
                onClick={() => void onPull?.()}
                disabled={!onPull || pullLoading || syncLoading}
                title={`Pull ${commitsBehind} commit${commitsBehind > 1 ? "s" : ""}`}
              >
                {pullLoading ? (
                  <span className="commit-button-spinner" aria-hidden />
                ) : (
                  <Download size={14} aria-hidden />
                )}
                <span>{pullLoading ? "Pulling..." : "Pull"}</span>
                <span className="push-count">{commitsBehind}</span>
              </button>
            )}
            {commitsAhead > 0 && (
              <button
                type="button"
                className="push-button"
                onClick={() => void onPush?.()}
                disabled={!onPush || pushLoading || commitsBehind > 0}
                title={
                  commitsBehind > 0
                    ? "Remote is ahead. Pull first, or use Sync."
                    : `Push ${commitsAhead} commit${commitsAhead > 1 ? "s" : ""}`
                }
              >
                {pushLoading ? (
                  <span className="commit-button-spinner" aria-hidden />
                ) : (
                  <Upload size={14} aria-hidden />
                )}
                <span>Push</span>
                <span className="push-count">{commitsAhead}</span>
              </button>
            )}
          </div>
          {commitsAhead > 0 && commitsBehind > 0 && (
            <button
              type="button"
              className="push-button-secondary"
              onClick={() => void onSync?.()}
              disabled={!onSync || syncLoading || pullLoading}
              title="Pull latest changes and push your local commits"
            >
              {syncLoading ? (
                <span className="commit-button-spinner" aria-hidden />
              ) : (
                <RotateCcw size={14} aria-hidden />
              )}
              <span>{syncLoading ? "Syncing..." : "Sync (pull then push)"}</span>
            </button>
          )}
        </div>
      )}
      {!error &&
        !stagedFiles.length &&
        !unstagedFiles.length &&
        commitsAhead === 0 &&
        commitsBehind === 0 && <div className="diff-empty">No changes detected.</div>}
      {(stagedFiles.length > 0 || unstagedFiles.length > 0) && (
        <>
          {stagedFiles.length > 0 && (
            <DiffSection
              title="Staged"
              files={stagedFiles}
              section="staged"
              selectedFiles={selectedFiles}
              selectedPath={selectedPath}
              onSelectFile={onSelectFile}
              onUnstageFile={onUnstageFile}
              onDiscardFile={onDiscardFile}
              onDiscardFiles={onDiscardFiles}
              showWorktreeApplyAction={showWorktreeApplyInStaged}
              worktreeApplyTitle={worktreeApplyTitle}
              worktreeApplyLoading={worktreeApplyLoading}
              worktreeApplySuccess={worktreeApplySuccess}
              onApplyWorktreeChanges={onApplyWorktreeChanges}
              onFileClick={onFileClick}
              onShowFileMenu={onShowFileMenu}
            />
          )}
          {unstagedFiles.length > 0 && (
            <DiffSection
              title="Unstaged"
              files={unstagedFiles}
              section="unstaged"
              selectedFiles={selectedFiles}
              selectedPath={selectedPath}
              onSelectFile={onSelectFile}
              onStageAllChanges={onStageAllChanges}
              onStageFile={onStageFile}
              onDiscardFile={onDiscardFile}
              onDiscardFiles={onDiscardFiles}
              onReviewUncommittedChanges={onReviewUncommittedChanges}
              showWorktreeApplyAction={showWorktreeApplyInUnstaged}
              worktreeApplyTitle={worktreeApplyTitle}
              worktreeApplyLoading={worktreeApplyLoading}
              worktreeApplySuccess={worktreeApplySuccess}
              onApplyWorktreeChanges={onApplyWorktreeChanges}
              onFileClick={onFileClick}
              onShowFileMenu={onShowFileMenu}
            />
          )}
        </>
      )}
    </div>
  );
}
