-- Workspace-wide source discovery for Thread-first data intake.
--
-- A Draft is an upload transaction only.  SourceAsset is owned by the
-- Workspace and is therefore visible to every authorized Thread in that
-- Workspace.  Intake state remains Task-scoped and records the evidence and
-- confirmations used for one analysis.

ALTER TABLE workspaces
    ADD COLUMN source_revision BIGINT NOT NULL DEFAULT 0 CHECK (source_revision >= 0);

-- A large MCP Resource remains the authoritative artifact content.  The
-- bounded intake envelope is stored separately so the platform can project
-- readiness without copying the business dataset into its intake state.
ALTER TABLE artifacts
    ADD COLUMN intake_envelope JSONB;

ALTER TABLE workspace_dataset_releases
    ADD CONSTRAINT workspace_dataset_releases_org_id_workspace_unique
    UNIQUE (organization_id, id, workspace_id);

CREATE TABLE workspace_data_drafts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL,
    owner_user_id   UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    revision        BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    state           TEXT NOT NULL DEFAULT 'active'
                        CHECK (state IN ('active', 'ready', 'failed', 'cancelled')),
    idempotency_key TEXT NOT NULL,
    failure_code    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, workspace_id, idempotency_key),
    UNIQUE (organization_id, id),
    UNIQUE (organization_id, id, workspace_id),
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE CASCADE
);

CREATE TABLE workspace_data_source_assets (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL,
    owner_user_id   UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    file_name       TEXT NOT NULL,
    normalized_file_name TEXT NOT NULL,
    media_type      TEXT NOT NULL,
    byte_size       BIGINT NOT NULL CHECK (byte_size BETWEEN 1 AND 104857600),
    content_sha256  TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    relative_path   TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, workspace_id, content_sha256, normalized_file_name),
    UNIQUE (organization_id, id),
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE CASCADE
);
CREATE INDEX workspace_data_source_assets_workspace
    ON workspace_data_source_assets(organization_id, workspace_id, created_at, id);

CREATE TABLE workspace_data_draft_assets (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    draft_id        UUID NOT NULL,
    asset_id        UUID NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (draft_id, asset_id),
    FOREIGN KEY (organization_id, draft_id)
        REFERENCES workspace_data_drafts(organization_id, id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, asset_id)
        REFERENCES workspace_data_source_assets(organization_id, id) ON DELETE RESTRICT
);
CREATE INDEX workspace_data_draft_assets_asset
    ON workspace_data_draft_assets(organization_id, asset_id);

CREATE TABLE data_intake_sessions (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    workspace_id          UUID NOT NULL,
    task_id               UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    thread_id             TEXT,
    contract_id           TEXT NOT NULL,
    contract_version      TEXT NOT NULL,
    contract_sha256       TEXT NOT NULL CHECK (contract_sha256 ~ '^[0-9a-f]{64}$'),
    status                TEXT NOT NULL DEFAULT 'active'
                            CHECK (status IN ('active', 'ready', 'failed', 'cancelled')),
    input_revision        BIGINT NOT NULL DEFAULT 0 CHECK (input_revision >= 0),
    mapping_revision      BIGINT NOT NULL DEFAULT 0 CHECK (mapping_revision >= 0),
    evidence_revision     BIGINT NOT NULL DEFAULT 0 CHECK (evidence_revision >= 0),
    gap_fingerprint       TEXT NOT NULL DEFAULT '',
    evidence_fingerprint  TEXT NOT NULL DEFAULT '',
    gaps                  JSONB NOT NULL DEFAULT '[]',
    blocked_reasons       JSONB NOT NULL DEFAULT '[]',
    mapping_candidates    JSONB NOT NULL DEFAULT '[]',
    confirmed_mapping     JSONB NOT NULL DEFAULT '[]',
    parameter_answers     JSONB NOT NULL DEFAULT '[]',
    requirement_profile   JSONB,
    source_profile        JSONB,
    mapping_proposal      JSONB,
    planning_dataset      JSONB,
    readiness_review      JSONB,
    requirement_artifact_id UUID,
    source_artifact_id      UUID,
    mapping_artifact_id     UUID,
    dataset_artifact_id     UUID,
    readiness_artifact_id   UUID,
    profile_confirmation_sha256 TEXT,
    mapping_confirmation_sha256 TEXT,
    parameter_confirmation_sha256 TEXT,
    readiness_confirmation_sha256 TEXT,
    normalized_release_id UUID,
    current_binding_id   UUID,
    idempotency_key       TEXT NOT NULL,
    attempt_count         INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    failure_code          TEXT,
    failure_summary       TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, task_id, contract_id, contract_version),
    UNIQUE (organization_id, id),
    UNIQUE (organization_id, id, task_id, workspace_id, contract_id, contract_version, contract_sha256),
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE CASCADE,
    FOREIGN KEY (organization_id, normalized_release_id, workspace_id)
        REFERENCES workspace_dataset_releases(organization_id, id, workspace_id) ON DELETE RESTRICT
);
CREATE INDEX data_intake_sessions_task
    ON data_intake_sessions(organization_id, task_id, updated_at DESC);

CREATE TABLE data_intake_input_requests (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id               UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    intake_session_id     UUID NOT NULL,
    kind                  TEXT NOT NULL CHECK (kind IN (
        'confirm_profile', 'confirm_mapping', 'answer_parameters',
        'provide_data', 'confirm_analysis'
    )),
    artifact_id           UUID,
    session_revision      BIGINT NOT NULL CHECK (session_revision >= 0),
    evidence_fingerprint  TEXT NOT NULL CHECK (evidence_fingerprint ~ '^[0-9a-f]{64}$'),
    idempotency_key       TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'open'
                            CHECK (status IN ('open', 'answered', 'superseded', 'cancelled')),
    response              JSONB,
    source_turn_id        TEXT,
    source_item_id        TEXT,
    response_idempotency_key TEXT,
    response_state         TEXT NOT NULL DEFAULT 'pending'
                            CHECK (response_state IN ('pending', 'starting', 'started', 'failed')),
    response_turn_id       TEXT,
    response_failure_code  TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    answered_at           TIMESTAMPTZ,
    UNIQUE (organization_id, task_id, idempotency_key),
    FOREIGN KEY (organization_id, intake_session_id)
        REFERENCES data_intake_sessions(organization_id, id) ON DELETE CASCADE
);
CREATE INDEX data_intake_input_requests_pending
    ON data_intake_input_requests(organization_id, task_id, status, created_at DESC);
CREATE UNIQUE INDEX data_intake_input_requests_open_evidence
    ON data_intake_input_requests(organization_id, task_id, kind, evidence_fingerprint)
    WHERE status = 'open';
CREATE UNIQUE INDEX data_intake_input_requests_response_key
    ON data_intake_input_requests(organization_id, task_id, response_idempotency_key)
    WHERE response_idempotency_key IS NOT NULL;

CREATE TABLE task_dataset_bindings (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id               UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    workspace_id          UUID NOT NULL,
    intake_session_id     UUID NOT NULL,
    dataset_release_id    UUID NOT NULL,
    requirement_profile_sha256 TEXT NOT NULL CHECK (requirement_profile_sha256 ~ '^[0-9a-f]{64}$'),
    source_snapshot_sha256    TEXT NOT NULL CHECK (source_snapshot_sha256 ~ '^[0-9a-f]{64}$'),
    contract_id           TEXT NOT NULL,
    contract_version      TEXT NOT NULL,
    contract_sha256       TEXT NOT NULL CHECK (contract_sha256 ~ '^[0-9a-f]{64}$'),
    mapping_sha256        TEXT NOT NULL CHECK (mapping_sha256 ~ '^[0-9a-f]{64}$'),
    parameter_sha256      TEXT NOT NULL CHECK (parameter_sha256 ~ '^[0-9a-f]{64}$'),
    dataset_sha256        TEXT NOT NULL CHECK (dataset_sha256 ~ '^[0-9a-f]{64}$'),
    snapshot              JSONB NOT NULL,
    fingerprint           TEXT NOT NULL CHECK (fingerprint ~ '^[0-9a-f]{64}$'),
    supersedes_binding_id UUID,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, id),
    UNIQUE (organization_id, task_id, fingerprint),
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE CASCADE,
    FOREIGN KEY (
        organization_id, intake_session_id, task_id, workspace_id,
        contract_id, contract_version, contract_sha256
    ) REFERENCES data_intake_sessions(
        organization_id, id, task_id, workspace_id,
        contract_id, contract_version, contract_sha256
    ) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, dataset_release_id, workspace_id)
        REFERENCES workspace_dataset_releases(organization_id, id, workspace_id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, supersedes_binding_id)
        REFERENCES task_dataset_bindings(organization_id, id) ON DELETE RESTRICT
);
CREATE INDEX task_dataset_bindings_current
    ON task_dataset_bindings(organization_id, task_id, created_at DESC);
CREATE INDEX task_dataset_bindings_release
    ON task_dataset_bindings(organization_id, dataset_release_id);

ALTER TABLE data_intake_sessions
    ADD CONSTRAINT data_intake_sessions_current_binding_fk
    FOREIGN KEY (organization_id, current_binding_id)
    REFERENCES task_dataset_bindings(organization_id, id) ON DELETE SET NULL;

CREATE TABLE task_analysis_execution_snapshots (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id               UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    thread_id             TEXT,
    run_id                UUID,
    turn_id               TEXT,
    binding_id            UUID NOT NULL,
    readiness_fingerprint TEXT NOT NULL CHECK (readiness_fingerprint ~ '^[0-9a-f]{64}$'),
    input_snapshot        JSONB NOT NULL,
    idempotency_key       TEXT NOT NULL,
    state                 TEXT NOT NULL DEFAULT 'created'
                            CHECK (state IN ('created', 'starting', 'started', 'failed', 'revoked')),
    failure_code          TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, task_id, idempotency_key),
    UNIQUE (organization_id, task_id, readiness_fingerprint),
    FOREIGN KEY (organization_id, binding_id)
        REFERENCES task_dataset_bindings(organization_id, id) ON DELETE RESTRICT
);

-- Evidence projection is deliberately separate from the Artifact content
-- store.  Resource materialisation and Runtime event replay can therefore
-- converge on the same intake projection without creating duplicate input
-- requests.
CREATE TABLE task_intake_artifact_projections (
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    artifact_id     UUID NOT NULL,
    envelope_sha256 TEXT NOT NULL CHECK (envelope_sha256 ~ '^[0-9a-f]{64}$'),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (organization_id, task_id, artifact_id, envelope_sha256)
);

-- The producer identity is taken from the authoritative Runtime/policy
-- projection, never from model-controlled Artifact JSON.
CREATE TABLE task_policy_agent_producers (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    task_id             UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    policy_snapshot_id  UUID NOT NULL,
    agent_thread_id     TEXT NOT NULL,
    agent_instance_id   TEXT,
    role_id             TEXT NOT NULL,
    release_hash        TEXT NOT NULL CHECK (release_hash ~ '^[0-9a-f]{64}$'),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, task_id, agent_thread_id),
    UNIQUE (organization_id, task_id, agent_instance_id)
);
CREATE INDEX task_policy_agent_producers_task
    ON task_policy_agent_producers(organization_id, task_id, created_at DESC);
