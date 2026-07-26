-- Rebuildable views of the Runtime-owned Thread tree.
--
-- Codex remains authoritative for Thread lifecycle, agent creation and
-- communication. These rows exist only so the platform can associate child
-- Thread events with an authorized root Run and restore a safe collaboration
-- view after browser reconnect.

CREATE TABLE runtime_agent_projections (
    organization_id    UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id         UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    workspace_id       UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    root_run_id        UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    thread_id          TEXT NOT NULL,
    parent_thread_id   TEXT,
    source_kind        TEXT NOT NULL,
    agent_path         TEXT,
    agent_nickname     TEXT,
    agent_role         TEXT,
    status_type        TEXT,
    active_flags       TEXT[] NOT NULL DEFAULT '{}',
    first_observed_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_observed_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (profile_id, thread_id),
    UNIQUE (root_run_id, thread_id),
    CHECK (length(thread_id) BETWEEN 1 AND 256),
    CHECK (parent_thread_id IS NULL OR length(parent_thread_id) BETWEEN 1 AND 256),
    CHECK (length(source_kind) BETWEEN 1 AND 64),
    CHECK (agent_path IS NULL OR length(agent_path) BETWEEN 1 AND 512),
    CHECK (agent_nickname IS NULL OR length(agent_nickname) BETWEEN 1 AND 128),
    CHECK (agent_role IS NULL OR length(agent_role) BETWEEN 1 AND 128),
    CHECK (status_type IS NULL OR length(status_type) BETWEEN 1 AND 64),
    CHECK (
        (source_kind = 'root' AND parent_thread_id IS NULL)
        OR (source_kind <> 'root' AND parent_thread_id IS NOT NULL)
    )
);

CREATE INDEX idx_runtime_agent_projections_run
    ON runtime_agent_projections(root_run_id, first_observed_at, thread_id);
CREATE INDEX idx_runtime_agent_projections_parent
    ON runtime_agent_projections(profile_id, parent_thread_id)
    WHERE parent_thread_id IS NOT NULL;
