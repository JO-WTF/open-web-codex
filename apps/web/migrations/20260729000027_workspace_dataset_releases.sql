-- Immutable, Workspace-scoped Dataset Releases. PostgreSQL owns publication
-- identity and lifecycle; uploaded bytes remain in the authorized Workspace.

ALTER TABLE workspaces
    ADD CONSTRAINT workspaces_organization_id_id_unique
    UNIQUE (organization_id, id);

CREATE TABLE workspace_dataset_releases (
    id                 UUID PRIMARY KEY,
    organization_id    UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id       UUID NOT NULL,
    owner_user_id      UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    idempotency_key    TEXT NOT NULL,
    dataset_id         TEXT NOT NULL,
    version            TEXT NOT NULL,
    display_name       TEXT NOT NULL,
    description        TEXT NOT NULL,
    state              TEXT NOT NULL
                           CHECK (state IN ('publishing', 'published', 'failed')),
    content_sha256     TEXT NOT NULL,
    failure_code       TEXT,
    published_at       TIMESTAMPTZ,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (dataset_id ~ '^[a-z0-9](?:[a-z0-9-]{0,62}[a-z0-9])?$'),
    CHECK (version ~ '^[A-Za-z0-9](?:[A-Za-z0-9._-]{0,62}[A-Za-z0-9])?$'),
    CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
    CHECK (length(trim(description)) BETWEEN 1 AND 1024),
    CHECK (length(idempotency_key) BETWEEN 8 AND 128),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    CHECK (
        (state = 'published' AND published_at IS NOT NULL AND failure_code IS NULL)
        OR (state = 'failed' AND published_at IS NULL AND failure_code IS NOT NULL)
        OR (state = 'publishing' AND published_at IS NULL AND failure_code IS NULL)
    ),
    UNIQUE (organization_id, workspace_id, idempotency_key),
    UNIQUE (workspace_id, dataset_id, version),
    UNIQUE (organization_id, id),
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE CASCADE
);

CREATE TABLE workspace_dataset_release_files (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    release_id      UUID NOT NULL,
    logical_name    TEXT NOT NULL,
    role            TEXT NOT NULL,
    media_type      TEXT NOT NULL,
    byte_size       BIGINT NOT NULL,
    content_sha256  TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(logical_name) BETWEEN 1 AND 256),
    CHECK (length(role) BETWEEN 1 AND 96),
    CHECK (length(media_type) BETWEEN 3 AND 160),
    CHECK (byte_size BETWEEN 0 AND 33554432),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    PRIMARY KEY (release_id, logical_name),
    FOREIGN KEY (organization_id, release_id)
        REFERENCES workspace_dataset_releases(organization_id, id) ON DELETE CASCADE
);

CREATE INDEX idx_workspace_dataset_releases_list
    ON workspace_dataset_releases(
        organization_id,
        workspace_id,
        created_at DESC
    );
