-- One catalog for Tool, Skill, Agent, Supervisor and Copilot packages.
-- Drafts are mutable integer revisions; Releases are immutable server-owned
-- identities. Runtime installation is a separate, observable projection.

CREATE TABLE capability_catalog_drafts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    owner_user_id   UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    kind            TEXT NOT NULL CHECK (kind IN (
                        'tool_package', 'skill_package', 'agent_definition',
                        'supervisor_definition', 'copilot_package'
                    )),
    resource_id     TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    description     TEXT NOT NULL,
    revision        BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
    content         JSONB NOT NULL CHECK (jsonb_typeof(content) = 'object'),
    content_sha256  TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    validation_state TEXT NOT NULL DEFAULT 'unvalidated'
                    CHECK (validation_state IN ('unvalidated', 'valid', 'invalid')),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, kind, resource_id)
);

CREATE INDEX capability_catalog_drafts_owner_idx
    ON capability_catalog_drafts(organization_id, owner_user_id, updated_at DESC);

CREATE TABLE capability_catalog_releases (
    id                         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id            UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    draft_id                   UUID NOT NULL REFERENCES capability_catalog_drafts(id) ON DELETE RESTRICT,
    kind                       TEXT NOT NULL CHECK (kind IN (
                        'tool_package', 'skill_package', 'agent_definition',
                        'supervisor_definition', 'copilot_package'
                    )),
    resource_id                TEXT NOT NULL,
    release_version            TEXT NOT NULL,
    display_name               TEXT NOT NULL,
    description                TEXT NOT NULL,
    content                    JSONB NOT NULL CHECK (jsonb_typeof(content) = 'object'),
    content_sha256             TEXT NOT NULL CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    execution_semantics_sha256 TEXT NOT NULL CHECK (execution_semantics_sha256 ~ '^[0-9a-f]{64}$'),
    published_by               UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    published_at               TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, kind, resource_id, release_version),
    UNIQUE (organization_id, id)
);

CREATE INDEX capability_catalog_releases_lookup_idx
    ON capability_catalog_releases(organization_id, kind, resource_id, published_at DESC);

CREATE TABLE capability_catalog_release_dependencies (
    release_id      UUID NOT NULL REFERENCES capability_catalog_releases(id) ON DELETE CASCADE,
    dependency_kind TEXT NOT NULL CHECK (dependency_kind IN (
                        'tool_package', 'skill_package', 'agent_definition',
                        'supervisor_definition', 'copilot_package'
                    )),
    dependency_resource_id TEXT NOT NULL,
    dependency_release_id UUID REFERENCES capability_catalog_releases(id) ON DELETE RESTRICT,
    dependency_release_version TEXT,
    PRIMARY KEY (release_id, dependency_kind, dependency_resource_id),
    CHECK (dependency_release_id IS NOT NULL OR dependency_release_version IS NOT NULL)
);

CREATE TABLE capability_catalog_installations (
    id                        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id           UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    release_id                UUID NOT NULL REFERENCES capability_catalog_releases(id) ON DELETE RESTRICT,
    workspace_id              UUID NOT NULL,
    profile_id                UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    state                     TEXT NOT NULL CHECK (state IN (
                                'authorized', 'installing', 'installed', 'discovered',
                                'ready', 'failed', 'uninstalled'
                            )),
    observed_content_sha256   TEXT CHECK (observed_content_sha256 IS NULL OR observed_content_sha256 ~ '^[0-9a-f]{64}$'),
    failure_code              TEXT,
    created_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at                TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (organization_id, release_id, workspace_id),
    FOREIGN KEY (workspace_id)
        REFERENCES workspaces(id) ON DELETE CASCADE
);

CREATE INDEX capability_catalog_installations_workspace_idx
    ON capability_catalog_installations(organization_id, workspace_id, updated_at DESC);

CREATE TABLE capability_catalog_installation_events (
    sequence       BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    installation_id UUID NOT NULL REFERENCES capability_catalog_installations(id) ON DELETE CASCADE,
    state          TEXT NOT NULL,
    content_sha256 TEXT,
    failure_code   TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
