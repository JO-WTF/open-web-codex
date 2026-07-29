-- Immutable root-Agent snapshots and their complete Run delivery lifecycle.

CREATE TABLE agent_run_snapshots (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    definition_id   TEXT NOT NULL,
    version         TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    content_sha256  TEXT NOT NULL,
    source          TEXT NOT NULL CHECK (source IN ('repository', 'user_release')),
    release_id      UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(trim(definition_id)) BETWEEN 1 AND 96),
    CHECK (length(trim(version)) BETWEEN 1 AND 64),
    CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    CHECK (
        (source = 'repository' AND release_id IS NULL)
        OR (source = 'user_release' AND release_id IS NOT NULL)
    ),
    UNIQUE (organization_id, definition_id, version),
    UNIQUE (organization_id, id),
    FOREIGN KEY (organization_id, release_id)
        REFERENCES agent_definition_releases(organization_id, id) ON DELETE RESTRICT
);

CREATE TABLE agent_run_bindings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    run_id          UUID NOT NULL UNIQUE REFERENCES runs(id) ON DELETE CASCADE,
    snapshot_id     UUID NOT NULL,
    thread_id       TEXT,
    state           TEXT NOT NULL DEFAULT 'prepared'
                         CHECK (state IN ('prepared', 'bound', 'failed', 'cancelled')),
    failure_code    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    bound_at        TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (state = 'prepared' AND thread_id IS NULL AND bound_at IS NULL)
        OR (state = 'bound' AND length(trim(thread_id)) BETWEEN 1 AND 256 AND bound_at IS NOT NULL)
        OR (state IN ('failed', 'cancelled') AND thread_id IS NULL AND bound_at IS NULL)
    ),
    FOREIGN KEY (organization_id, snapshot_id)
        REFERENCES agent_run_snapshots(organization_id, id) ON DELETE RESTRICT
);

CREATE UNIQUE INDEX idx_agent_run_binding_thread
    ON agent_run_bindings(profile_id, thread_id)
    WHERE thread_id IS NOT NULL;

CREATE INDEX idx_agent_run_binding_task
    ON agent_run_bindings(organization_id, task_id, created_at);
