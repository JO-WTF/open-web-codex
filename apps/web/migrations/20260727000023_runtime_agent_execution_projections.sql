-- Durable, rebuildable browser projections of child Agent task executions.
--
-- Codex remains authoritative for Agent Thread and Turn lifecycle. Each row
-- corresponds to one observed child Turn (or a spawn waiting for its first
-- Turn) and is updated only from persisted Runtime events.

CREATE TABLE runtime_agent_execution_projections (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id          UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id               UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    workspace_id             UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    root_run_id              UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    agent_thread_id          TEXT NOT NULL,
    turn_id                  TEXT,
    ordinal                  INTEGER NOT NULL,
    assignment_sequence      BIGINT,
    assignment_item_id       TEXT,
    task                     TEXT,
    status                   TEXT NOT NULL,
    current_behavior         TEXT NOT NULL,
    latest_progress          TEXT,
    first_observed_sequence  BIGINT NOT NULL,
    last_observed_sequence   BIGINT NOT NULL,
    started_at               TIMESTAMPTZ,
    completed_at             TIMESTAMPTZ,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (root_run_id, agent_thread_id, ordinal),
    UNIQUE (root_run_id, agent_thread_id, turn_id),
    UNIQUE (root_run_id, agent_thread_id, assignment_item_id),
    CHECK (length(agent_thread_id) BETWEEN 1 AND 256),
    CHECK (turn_id IS NULL OR length(turn_id) BETWEEN 1 AND 256),
    CHECK (assignment_item_id IS NULL OR length(assignment_item_id) BETWEEN 1 AND 256),
    CHECK (ordinal > 0),
    CHECK (task IS NULL OR length(task) BETWEEN 1 AND 1000),
    CHECK (status IN ('pending', 'running', 'waiting', 'completed', 'failed', 'interrupted')),
    CHECK (length(current_behavior) BETWEEN 1 AND 500),
    CHECK (latest_progress IS NULL OR length(latest_progress) BETWEEN 1 AND 1000),
    CHECK (last_observed_sequence >= first_observed_sequence)
);

CREATE INDEX idx_runtime_agent_execution_projections_run
    ON runtime_agent_execution_projections(root_run_id, first_observed_sequence, id);
CREATE INDEX idx_runtime_agent_execution_projections_active
    ON runtime_agent_execution_projections(root_run_id, agent_thread_id, ordinal DESC)
    WHERE status IN ('pending', 'running', 'waiting');
