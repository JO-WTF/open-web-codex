-- Immutable Workspace-published capability packages. Runtime discovery remains
-- in Codex; this table owns platform publication identity and authorization.

CREATE TABLE workspace_capability_package_releases (
    id                  UUID PRIMARY KEY,
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id        UUID NOT NULL,
    owner_user_id       UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    idempotency_key     TEXT NOT NULL,
    package_id          TEXT NOT NULL,
    version             TEXT NOT NULL,
    display_name        TEXT NOT NULL,
    description         TEXT NOT NULL,
    state               TEXT NOT NULL
                            CHECK (state IN ('publishing', 'published', 'failed')),
    capability_root_id  TEXT NOT NULL,
    server_name         TEXT NOT NULL,
    skill_name          TEXT NOT NULL,
    tool_names          JSONB NOT NULL,
    capabilities        JSONB NOT NULL,
    input_artifact_types JSONB NOT NULL,
    output_artifact_types JSONB NOT NULL,
    content_sha256      TEXT NOT NULL,
    failure_code        TEXT,
    published_at        TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (package_id ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
    CHECK (version ~ '^[0-9](?:[0-9.]{0,62}[0-9])?$'),
    CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
    CHECK (length(trim(description)) BETWEEN 1 AND 500),
    CHECK (length(idempotency_key) BETWEEN 8 AND 128),
    CHECK (capability_root_id ~ '^local-[a-z0-9-]+$'),
    CHECK (server_name ~ '^[A-Za-z_][A-Za-z0-9_]{0,63}$'),
    CHECK (skill_name ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
    CHECK (jsonb_typeof(tool_names) = 'array'),
    CHECK (jsonb_array_length(tool_names) BETWEEN 1 AND 16),
    CHECK (jsonb_typeof(capabilities) = 'array'),
    CHECK (jsonb_array_length(capabilities) BETWEEN 1 AND 16),
    CHECK (jsonb_typeof(input_artifact_types) = 'array'),
    CHECK (jsonb_array_length(input_artifact_types) BETWEEN 0 AND 32),
    CHECK (jsonb_typeof(output_artifact_types) = 'array'),
    CHECK (jsonb_array_length(output_artifact_types) BETWEEN 1 AND 32),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    CHECK (
        (state = 'published' AND published_at IS NOT NULL AND failure_code IS NULL)
        OR (state = 'failed' AND published_at IS NULL AND failure_code IS NOT NULL)
        OR (state = 'publishing' AND published_at IS NULL AND failure_code IS NULL)
    ),
    UNIQUE (organization_id, workspace_id, idempotency_key),
    UNIQUE (workspace_id, package_id, version),
    UNIQUE (organization_id, id),
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_workspace_capability_package_releases_catalog
    ON workspace_capability_package_releases(
        organization_id,
        state,
        published_at DESC
    );
