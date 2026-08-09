-- A Task chooses its authorized Workspace exactly once.  Runs inherit that
-- choice and may not be retargeted by a browser request, replay, or fork.
--
-- This migration intentionally has no historical backfill. Development
-- databases containing Tasks or Runs from the retired contract must be
-- rebuilt with the current schema rather than guessed into a Workspace.

ALTER TABLE tasks ADD COLUMN workspace_id UUID;

ALTER TABLE tasks
    ADD CONSTRAINT tasks_workspace_id_fkey
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE RESTRICT;

ALTER TABLE tasks
    ADD CONSTRAINT tasks_id_workspace_id_key UNIQUE (id, workspace_id);

ALTER TABLE tasks ALTER COLUMN workspace_id SET NOT NULL;

ALTER TABLE runs DROP CONSTRAINT IF EXISTS runs_active_workspace_check;
ALTER TABLE runs ALTER COLUMN workspace_id SET NOT NULL;

ALTER TABLE runs DROP CONSTRAINT IF EXISTS runs_workspace_id_fkey;
ALTER TABLE runs
    ADD CONSTRAINT runs_workspace_id_fkey
    FOREIGN KEY (workspace_id) REFERENCES workspaces(id) ON DELETE RESTRICT;

ALTER TABLE runs
    ADD CONSTRAINT runs_task_workspace_id_fkey
    FOREIGN KEY (task_id, workspace_id)
    REFERENCES tasks(id, workspace_id) ON DELETE CASCADE;
