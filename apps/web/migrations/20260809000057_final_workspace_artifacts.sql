-- Final Artifacts are copied from an exact completed Tool Item's authorized
-- Workspace file. Intermediate MCP Resources are never promoted to Artifacts.

DROP TABLE inline_visualization_artifacts;
DROP TABLE artifact_task_grants;
DROP TABLE artifact_provenance;
DROP TABLE artifacts;

CREATE TABLE artifacts (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id           UUID NOT NULL REFERENCES profiles(id),
    workspace_id         UUID NOT NULL REFERENCES workspaces(id),
    artifact_schema      TEXT NOT NULL,
    display_name         TEXT NOT NULL,
    mime_type            TEXT NOT NULL,
    source_relative_path TEXT NOT NULL,
    expected_size        BIGINT NOT NULL CHECK (expected_size BETWEEN 1 AND 104857600),
    byte_size            BIGINT,
    content              BYTEA,
    content_sha256       TEXT,
    state                TEXT NOT NULL DEFAULT 'pending'
                             CHECK (state IN ('pending', 'materializing', 'ready', 'failed')),
    failure_code         TEXT,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, workspace_id, source_relative_path),
    CHECK (byte_size IS NULL OR byte_size BETWEEN 1 AND 104857600),
    CHECK (
        (state = 'ready' AND content IS NOT NULL AND byte_size IS NOT NULL
            AND content_sha256 IS NOT NULL AND failure_code IS NULL)
        OR (state = 'failed' AND content IS NULL AND byte_size IS NULL
            AND content_sha256 IS NULL AND failure_code IS NOT NULL)
        OR (state IN ('pending', 'materializing') AND content IS NULL
            AND byte_size IS NULL AND content_sha256 IS NULL AND failure_code IS NULL)
    )
);

CREATE TABLE artifact_task_grants (
    artifact_id     UUID NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id          UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    permission       TEXT NOT NULL DEFAULT 'read' CHECK (permission = 'read'),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (artifact_id, task_id)
);

CREATE TABLE artifact_provenance (
    artifact_id        UUID NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    organization_id    UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    producer_task_id   UUID NOT NULL,
    producer_run_id    UUID NOT NULL,
    producer_thread_id TEXT NOT NULL,
    producer_turn_id   TEXT NOT NULL,
    producer_item_id   TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        artifact_id,
        producer_run_id,
        producer_thread_id,
        producer_turn_id,
        producer_item_id
    ),
    UNIQUE (
        organization_id,
        producer_run_id,
        producer_thread_id,
        producer_turn_id,
        producer_item_id
    )
);

CREATE INDEX idx_artifact_task_grants_task
    ON artifact_task_grants(organization_id, task_id, created_at);

CREATE INDEX idx_artifact_provenance_producer
    ON artifact_provenance(
        organization_id,
        producer_run_id,
        producer_thread_id,
        producer_item_id
    );

CREATE INDEX idx_artifacts_materialization
    ON artifacts(state, created_at)
    WHERE state IN ('pending', 'materializing');
