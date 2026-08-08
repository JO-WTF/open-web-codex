-- Generic, durable work state for domain Copilots. Codex retains Thread,
-- Turn, Tool and Agent lifecycle ownership; this schema stores only the
-- platform-owned domain work projection and its immutable resource references.

CREATE TABLE work_state_definitions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID REFERENCES organizations(id) ON DELETE CASCADE,
    definition_id   TEXT NOT NULL,
    version         TEXT NOT NULL,
    content_sha256  TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    component_schema JSONB NOT NULL CHECK (jsonb_typeof(component_schema) = 'array'),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, definition_id, version),
    UNIQUE (organization_id, definition_id, content_sha256)
);

CREATE TABLE work_states (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id      UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    workspace_id    UUID NOT NULL,
    task_id         UUID NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    definition_id   UUID NOT NULL REFERENCES work_state_definitions(id) ON DELETE RESTRICT,
    state           TEXT NOT NULL DEFAULT 'active'
                        CHECK (state IN ('active', 'completed', 'failed', 'cancelled')),
    revision        BIGINT NOT NULL DEFAULT 0 CHECK (revision >= 0),
    idempotency_key TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, task_id, idempotency_key),
    UNIQUE (organization_id, id),
    FOREIGN KEY (workspace_id)
        REFERENCES workspaces(id) ON DELETE CASCADE
);
CREATE INDEX work_states_task_idx
    ON work_states(organization_id, task_id, updated_at DESC);

CREATE TABLE work_components (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    work_state_id   UUID NOT NULL REFERENCES work_states(id) ON DELETE CASCADE,
    component_key   TEXT NOT NULL,
    revision        BIGINT NOT NULL CHECK (revision > 0),
    state           TEXT NOT NULL CHECK (state IN ('missing', 'ready', 'invalidated', 'failed')),
    resource_ref    JSONB,
    summary         TEXT,
    operation_id    UUID,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (work_state_id, component_key, revision),
    CHECK (length(component_key) BETWEEN 1 AND 128),
    CHECK (summary IS NULL OR length(summary) <= 2000),
    CHECK (resource_ref IS NULL OR jsonb_typeof(resource_ref) = 'object')
);
CREATE INDEX work_components_current_idx
    ON work_components(work_state_id, component_key, revision DESC);

CREATE TABLE work_component_dependencies (
    work_state_id       UUID NOT NULL REFERENCES work_states(id) ON DELETE CASCADE,
    component_key       TEXT NOT NULL,
    depends_on_component_key TEXT NOT NULL,
    PRIMARY KEY (work_state_id, component_key, depends_on_component_key),
    CHECK (component_key <> depends_on_component_key)
);

CREATE TABLE work_operations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    work_state_id   UUID NOT NULL REFERENCES work_states(id) ON DELETE CASCADE,
    kind            TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN (
                        'pending', 'running', 'completed', 'failed', 'rejected',
                        'cancelled', 'timeout', 'interrupted'
                    )),
    idempotency_key TEXT NOT NULL,
    summary         TEXT,
    failure_code    TEXT,
    started_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    terminal_at     TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (work_state_id, idempotency_key),
    CHECK (length(kind) BETWEEN 1 AND 128),
    CHECK (summary IS NULL OR length(summary) <= 2000),
    CHECK ((terminal_at IS NULL) = (status IN ('pending', 'running')))
);
ALTER TABLE work_components
    ADD CONSTRAINT work_components_operation_fk
    FOREIGN KEY (operation_id) REFERENCES work_operations(id) ON DELETE SET NULL;

CREATE TABLE work_operation_inputs (
    operation_id  UUID NOT NULL REFERENCES work_operations(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    resource_ref  JSONB NOT NULL CHECK (jsonb_typeof(resource_ref) = 'object'),
    PRIMARY KEY (operation_id, name),
    CHECK (length(name) BETWEEN 1 AND 128)
);

CREATE TABLE work_operation_outputs (
    operation_id  UUID NOT NULL REFERENCES work_operations(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    resource_ref  JSONB NOT NULL CHECK (jsonb_typeof(resource_ref) = 'object'),
    PRIMARY KEY (operation_id, name),
    CHECK (length(name) BETWEEN 1 AND 128)
);

CREATE TABLE work_blocking_inputs (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    work_state_id       UUID NOT NULL REFERENCES work_states(id) ON DELETE CASCADE,
    source_operation_id UUID REFERENCES work_operations(id) ON DELETE SET NULL,
    code                TEXT NOT NULL,
    prompt              TEXT NOT NULL,
    state               TEXT NOT NULL DEFAULT 'open'
                            CHECK (state IN ('open', 'answered', 'cancelled')),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at         TIMESTAMPTZ,
    CHECK (length(code) BETWEEN 1 AND 128),
    CHECK (length(prompt) BETWEEN 1 AND 1000)
);
CREATE INDEX work_blocking_inputs_open_idx
    ON work_blocking_inputs(work_state_id, created_at) WHERE state = 'open';

CREATE TABLE work_deliverables (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    work_state_id   UUID NOT NULL REFERENCES work_states(id) ON DELETE CASCADE,
    operation_id    UUID REFERENCES work_operations(id) ON DELETE SET NULL,
    schema          TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    resource_ref    JSONB NOT NULL CHECK (jsonb_typeof(resource_ref) = 'object'),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(schema) BETWEEN 1 AND 256),
    CHECK (length(display_name) BETWEEN 1 AND 256)
);
CREATE INDEX work_deliverables_state_idx ON work_deliverables(work_state_id, created_at DESC);

CREATE TABLE work_state_events (
    sequence        BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    work_state_id   UUID NOT NULL REFERENCES work_states(id) ON DELETE CASCADE,
    operation_id    UUID REFERENCES work_operations(id) ON DELETE SET NULL,
    event_type      TEXT NOT NULL,
    revision        BIGINT NOT NULL CHECK (revision >= 0),
    summary         TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (length(event_type) BETWEEN 1 AND 128),
    CHECK (summary IS NULL OR length(summary) <= 2000)
);
CREATE INDEX work_state_events_state_idx ON work_state_events(work_state_id, sequence);
