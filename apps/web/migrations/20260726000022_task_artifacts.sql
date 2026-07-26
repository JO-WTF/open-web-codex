-- Durable enterprise Artifacts have their own identity and authorization.
-- Runtime Run/Thread/Turn/Item identifiers are provenance, never ownership.

DROP TABLE reply_artifacts;

CREATE TABLE artifacts (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id          UUID NOT NULL REFERENCES profiles(id),
    artifact_schema     TEXT NOT NULL,
    display_name        TEXT NOT NULL,
    mime_type           TEXT NOT NULL,
    expected_size       BIGINT,
    byte_size           BIGINT,
    content             BYTEA,
    content_sha256      TEXT,
    source_server       TEXT NOT NULL,
    source_uri          TEXT NOT NULL,
    state               TEXT NOT NULL DEFAULT 'pending'
                            CHECK (state IN ('pending', 'materializing', 'ready', 'failed')),
    failure_code        TEXT,
    retention_state     TEXT NOT NULL DEFAULT 'active'
                            CHECK (retention_state IN ('active', 'expired')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, profile_id, source_server, source_uri),
    CHECK (expected_size IS NULL OR expected_size >= 0),
    CHECK (byte_size IS NULL OR byte_size >= 0),
    CHECK (
        (state = 'ready' AND content IS NOT NULL AND byte_size IS NOT NULL
            AND content_sha256 IS NOT NULL AND failure_code IS NULL)
        OR (state = 'failed' AND content IS NULL AND failure_code IS NOT NULL)
        OR (state IN ('pending', 'materializing') AND content IS NULL
            AND byte_size IS NULL AND content_sha256 IS NULL AND failure_code IS NULL)
    )
);

CREATE TABLE artifact_task_grants (
    artifact_id         UUID NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id              UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    permission           TEXT NOT NULL DEFAULT 'read' CHECK (permission = 'read'),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (artifact_id, task_id)
);

CREATE TABLE artifact_provenance (
    artifact_id         UUID NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    -- Keep the immutable producer identity as provenance even if the producing
    -- Run is later removed under its own retention policy. A Run never owns
    -- the Artifact or its provenance row.
    producer_run_id     UUID NOT NULL,
    producer_thread_id  TEXT NOT NULL,
    producer_turn_id    TEXT NOT NULL,
    producer_item_id    TEXT NOT NULL,
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (
        artifact_id,
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
