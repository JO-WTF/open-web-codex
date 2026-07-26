-- Immutable, platform-owned Supervisor Policy snapshots bound to root Codex
-- Threads. Run and Task ids retain provenance; the binding remains explicit
-- even though Codex owns the model-visible Thread history.

CREATE TABLE supervisor_policy_snapshots (
    id                     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    policy_id              TEXT NOT NULL,
    version                TEXT NOT NULL,
    display_name           TEXT NOT NULL,
    developer_instructions TEXT NOT NULL,
    content_sha256         TEXT NOT NULL,
    source                 TEXT NOT NULL DEFAULT 'repository'
                               CHECK (source IN ('repository')),
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(trim(policy_id)) BETWEEN 1 AND 128),
    CHECK (length(trim(version)) BETWEEN 1 AND 64),
    CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
    CHECK (length(trim(developer_instructions)) BETWEEN 1 AND 16384),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    UNIQUE (organization_id, policy_id, version)
);

CREATE TABLE supervisor_policy_bindings (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    run_id          UUID NOT NULL UNIQUE REFERENCES runs(id) ON DELETE CASCADE,
    snapshot_id     UUID NOT NULL REFERENCES supervisor_policy_snapshots(id) ON DELETE RESTRICT,
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
    )
);

CREATE UNIQUE INDEX idx_supervisor_policy_binding_thread
    ON supervisor_policy_bindings(profile_id, thread_id)
    WHERE thread_id IS NOT NULL;

CREATE INDEX idx_supervisor_policy_binding_task
    ON supervisor_policy_bindings(organization_id, task_id, created_at);
