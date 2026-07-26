-- Browser projections for independently authorized Workspaces.

ALTER TABLE runs ADD COLUMN IF NOT EXISTS fork_thread_id TEXT;
ALTER TABLE runs ADD COLUMN IF NOT EXISTS fork_source_run_id UUID REFERENCES runs(id) ON DELETE SET NULL;

CREATE TABLE IF NOT EXISTS terminal_sessions (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id   UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id      UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id            UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    terminal_id       TEXT NOT NULL,
    process_id        TEXT NOT NULL UNIQUE,
    state             TEXT NOT NULL DEFAULT 'starting'
                          CHECK (state IN ('starting', 'running', 'closing', 'closed', 'failed')),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, workspace_id, terminal_id)
);

CREATE INDEX IF NOT EXISTS idx_terminal_sessions_active
    ON terminal_sessions(organization_id, workspace_id, updated_at DESC)
    WHERE state IN ('starting', 'running', 'closing');

-- Per-user browser presentation and safe Workspace automation state.
CREATE TABLE IF NOT EXISTS browser_workspace_preferences (
    organization_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id                UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    workspace_id           UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    settings               JSONB NOT NULL DEFAULT '{}'::jsonb,
    runtime_codex_args     TEXT,
    setup_completed_script TEXT,
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, user_id, workspace_id)
);

CREATE INDEX IF NOT EXISTS idx_browser_workspace_preferences_user
    ON browser_workspace_preferences(organization_id, user_id, updated_at DESC);
