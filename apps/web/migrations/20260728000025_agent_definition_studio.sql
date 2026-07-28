-- Organization-scoped Agent Definition governance. Runtime Role names and MCP
-- inventory are derived by the server from reviewed capability templates.

CREATE TABLE agent_definitions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    owner_user_id   UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    definition_id   TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    description     TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (definition_id ~ '^[a-z0-9][a-z0-9_-]{0,94}[a-z0-9]$'),
    CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
    CHECK (length(trim(description)) BETWEEN 1 AND 512),
    UNIQUE (organization_id, definition_id),
    UNIQUE (organization_id, id)
);

CREATE TABLE agent_definition_revisions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    definition_id   UUID NOT NULL,
    version         TEXT NOT NULL,
    state           TEXT NOT NULL DEFAULT 'draft'
                         CHECK (state IN ('draft', 'published')),
    draft_spec      JSONB NOT NULL,
    created_by      UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    published_at    TIMESTAMPTZ,
    CHECK (version ~ '^[a-z0-9][a-z0-9._-]{0,62}[a-z0-9]$'),
    CHECK (
        (state = 'draft' AND published_at IS NULL)
        OR (state = 'published' AND published_at IS NOT NULL)
    ),
    UNIQUE (definition_id, version),
    UNIQUE (organization_id, id),
    UNIQUE (organization_id, definition_id, id),
    FOREIGN KEY (organization_id, definition_id)
        REFERENCES agent_definitions(organization_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_agent_definition_revision_one_draft
    ON agent_definition_revisions(definition_id)
    WHERE state = 'draft';

CREATE TABLE agent_definition_releases (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    definition_id   UUID NOT NULL,
    revision_id     UUID NOT NULL UNIQUE,
    catalog_id      TEXT NOT NULL,
    version         TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    description     TEXT NOT NULL,
    release_spec    JSONB NOT NULL,
    runtime_role    TEXT NOT NULL,
    content_sha256  TEXT NOT NULL,
    published_by    UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    published_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (runtime_role ~ '^agent_[a-z0-9_]{1,57}$'),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    UNIQUE (organization_id, catalog_id, version),
    UNIQUE (organization_id, id),
    FOREIGN KEY (organization_id, definition_id)
        REFERENCES agent_definitions(organization_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, definition_id, revision_id)
        REFERENCES agent_definition_revisions(organization_id, definition_id, id)
        ON DELETE RESTRICT
);

CREATE TABLE supervisor_release_agent_dependencies (
    organization_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    supervisor_release_id UUID NOT NULL,
    agent_release_id      UUID NOT NULL,
    agent_definition_id   TEXT NOT NULL,
    agent_version         TEXT NOT NULL,
    agent_content_sha256  TEXT NOT NULL,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (supervisor_release_id, agent_release_id),
    CHECK (agent_content_sha256 ~ '^[0-9a-f]{64}$'),
    FOREIGN KEY (organization_id, supervisor_release_id)
        REFERENCES supervisor_releases(organization_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, agent_release_id)
        REFERENCES agent_definition_releases(organization_id, id) ON DELETE RESTRICT
);

CREATE INDEX idx_agent_definitions_owner
    ON agent_definitions(organization_id, owner_user_id, updated_at DESC);
CREATE INDEX idx_agent_definition_releases_catalog
    ON agent_definition_releases(organization_id, published_at DESC);
