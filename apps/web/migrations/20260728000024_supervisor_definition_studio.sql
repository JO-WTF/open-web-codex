-- User-authored Supervisor governance resources. Definitions and draft
-- revisions remain platform state; immutable Releases are resolved into Codex
-- Runtime contracts only after server-side validation.

CREATE TABLE supervisor_definitions (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    owner_user_id   UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    policy_id       TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    description     TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (policy_id ~ '^[a-z0-9][a-z0-9_-]{0,94}[a-z0-9]$'),
    CHECK (length(trim(display_name)) BETWEEN 1 AND 160),
    CHECK (length(trim(description)) BETWEEN 1 AND 512),
    UNIQUE (organization_id, policy_id),
    UNIQUE (organization_id, id)
);

CREATE TABLE supervisor_revisions (
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
        REFERENCES supervisor_definitions(organization_id, id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX idx_supervisor_revision_one_draft
    ON supervisor_revisions(definition_id)
    WHERE state = 'draft';

CREATE TABLE supervisor_releases (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    definition_id   UUID NOT NULL,
    revision_id     UUID NOT NULL UNIQUE,
    policy_id       TEXT NOT NULL,
    version         TEXT NOT NULL,
    display_name    TEXT NOT NULL,
    description     TEXT NOT NULL,
    release_spec    JSONB NOT NULL,
    content_sha256  TEXT NOT NULL,
    published_by    UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    published_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    UNIQUE (organization_id, policy_id, version),
    UNIQUE (organization_id, id),
    FOREIGN KEY (organization_id, definition_id)
        REFERENCES supervisor_definitions(organization_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, definition_id, revision_id)
        REFERENCES supervisor_revisions(organization_id, definition_id, id) ON DELETE RESTRICT
);

CREATE INDEX idx_supervisor_definitions_owner
    ON supervisor_definitions(organization_id, owner_user_id, updated_at DESC);
CREATE INDEX idx_supervisor_releases_catalog
    ON supervisor_releases(organization_id, published_at DESC);

ALTER TABLE supervisor_policy_snapshots
    ADD COLUMN release_id UUID REFERENCES supervisor_releases(id) ON DELETE RESTRICT;

ALTER TABLE supervisor_policy_snapshots
    DROP CONSTRAINT IF EXISTS supervisor_policy_snapshots_source_check;

ALTER TABLE supervisor_policy_snapshots
    ADD CONSTRAINT supervisor_policy_snapshots_source_check
    CHECK (
        (source = 'repository' AND release_id IS NULL)
        OR (source = 'user_release' AND release_id IS NOT NULL)
    );

ALTER TABLE supervisor_policy_snapshots
    ADD CONSTRAINT supervisor_policy_snapshots_release_scope_fk
    FOREIGN KEY (organization_id, release_id)
    REFERENCES supervisor_releases(organization_id, id) ON DELETE RESTRICT;
