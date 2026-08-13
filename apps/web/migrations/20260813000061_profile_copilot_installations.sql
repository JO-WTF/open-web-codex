-- One explicitly application-registered local/private Copilot installation per
-- Profile. This is desired/configured state only: Runtime readiness remains a
-- process-instance observation and is never persisted as installation truth.

CREATE TABLE profile_copilot_installations (
    profile_id             UUID PRIMARY KEY REFERENCES profiles(id) ON DELETE CASCADE,
    package_id             TEXT NOT NULL CHECK (
                               package_id ~ '^[a-z0-9](?:[a-z0-9_-]{0,94}[a-z0-9])?$'
                           ),
    desired_active         BOOLEAN NOT NULL DEFAULT FALSE,
    source_revision        TEXT NOT NULL CHECK (source_revision ~ '^[0-9a-f]{64}$'),
    configured_revision    TEXT CHECK (
                               configured_revision IS NULL
                               OR configured_revision ~ '^[0-9a-f]{64}$'
                           ),
    managed_skill_ids      TEXT[] NOT NULL DEFAULT '{}',
    managed_agent_role_ids TEXT[] NOT NULL DEFAULT '{}',
    last_failure_kind      TEXT CHECK (
                               last_failure_kind IS NULL
                               OR last_failure_kind IN ('unavailable', 'failed')
                           ),
    last_failure_code      TEXT CHECK (
                               last_failure_code IS NULL
                               OR last_failure_code ~ '^[a-z0-9_]{1,96}$'
                           ),
    installed_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (
        (last_failure_kind IS NULL AND last_failure_code IS NULL)
        OR (last_failure_kind IS NOT NULL AND last_failure_code IS NOT NULL)
    )
);

CREATE INDEX profile_copilot_installations_active_idx
    ON profile_copilot_installations(profile_id)
    WHERE desired_active;
