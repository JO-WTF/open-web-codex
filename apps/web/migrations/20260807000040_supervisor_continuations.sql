-- Persist the one continuation that a governed Supervisor may need when its
-- root Turn ends before all child Agents have reached a terminal state.
-- This is current Run lifecycle state, not a compatibility adapter for old
-- Runtime sessions.
CREATE TABLE IF NOT EXISTS supervisor_run_continuations (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id   UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    run_id            UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    workspace_id      UUID NOT NULL REFERENCES workspaces(id) ON DELETE RESTRICT,
    root_thread_id    TEXT NOT NULL,
    kind              TEXT NOT NULL CHECK (kind = 'child_completion'),
    status            TEXT NOT NULL DEFAULT 'pending'
                         CHECK (status IN ('pending', 'sending', 'sent', 'failed')),
    attempt           INTEGER NOT NULL DEFAULT 0 CHECK (attempt >= 0),
    turn_id           TEXT,
    failure_code      TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, kind),
    CHECK (length(root_thread_id) BETWEEN 1 AND 256),
    CHECK (turn_id IS NULL OR length(turn_id) BETWEEN 1 AND 256),
    CHECK (failure_code IS NULL OR length(failure_code) BETWEEN 1 AND 128)
);

CREATE INDEX IF NOT EXISTS idx_supervisor_run_continuations_pending
    ON supervisor_run_continuations(status, updated_at)
    WHERE status IN ('pending', 'sending');
