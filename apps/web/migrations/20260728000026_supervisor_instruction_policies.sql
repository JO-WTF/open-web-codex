-- Immutable platform-managed Supervisor behavior contracts. These are global
-- deployment policy resources, not organization-authored Supervisor content.

CREATE TABLE supervisor_instruction_policy_releases (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    policy_id             TEXT NOT NULL,
    version               TEXT NOT NULL,
    display_name          TEXT NOT NULL,
    description           TEXT NOT NULL,
    platform_instructions TEXT NOT NULL,
    content_sha256        TEXT NOT NULL,
    published_by          UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    published_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (policy_id ~ '^[a-z0-9][a-z0-9_-]{0,94}[a-z0-9]$'),
    CHECK (version ~ '^[a-z0-9][a-z0-9._-]{0,62}[a-z0-9]$'),
    CHECK (length(trim(display_name)) BETWEEN 1 AND 256),
    CHECK (length(trim(description)) BETWEEN 1 AND 512),
    CHECK (length(trim(platform_instructions)) BETWEEN 1 AND 16384),
    CHECK (content_sha256 ~ '^[0-9a-f]{64}$'),
    UNIQUE (policy_id, version)
);

CREATE INDEX idx_supervisor_instruction_policy_releases_published
    ON supervisor_instruction_policy_releases(published_at DESC, policy_id, version);
