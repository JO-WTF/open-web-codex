-- Platform-managed Agent Definitions are governance facts. This table first
-- durably reserves an intended Profile Runtime Role projection, then records
-- the successful Runtime configuration read-back. A NULL verified_at is a
-- retryable pending intent, never a verified Runtime Role. This is not a
-- Runtime Agent execution-state table.

CREATE TABLE IF NOT EXISTS profile_runtime_role_projections (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id          UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    definition_id       TEXT NOT NULL,
    definition_version  TEXT NOT NULL,
    runtime_role        TEXT NOT NULL,
    config_file         TEXT NOT NULL,
    content_sha256      TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    verified_at         TIMESTAMPTZ,
    CHECK (length(definition_id) BETWEEN 1 AND 256),
    CHECK (length(definition_version) BETWEEN 1 AND 128),
    CHECK (runtime_role ~ '^[a-z0-9_]+$'),
    CHECK (config_file ~ '^platform-agents/[A-Za-z0-9_-]+/[A-Za-z0-9._-]+\.toml$'),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    UNIQUE (profile_id, runtime_role),
    UNIQUE (profile_id, definition_id, definition_version)
);

CREATE INDEX IF NOT EXISTS idx_profile_runtime_role_projections_organization
    ON profile_runtime_role_projections (organization_id, profile_id, verified_at DESC);
