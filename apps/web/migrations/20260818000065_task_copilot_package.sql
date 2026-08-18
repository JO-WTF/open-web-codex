-- A Task fixes one application-discovered Copilot package for the lifetime of
-- its Root Thread. NULL remains valid only when the deployment has no available
-- Copilot packages; paths and Runtime configuration are never persisted here.

ALTER TABLE profile_copilot_installations
    DROP CONSTRAINT profile_copilot_installations_pkey,
    ADD PRIMARY KEY (profile_id, package_id);

ALTER TABLE tasks
    ADD COLUMN copilot_package_id TEXT;

ALTER TABLE tasks
    ADD CONSTRAINT tasks_copilot_package_check CHECK (
        copilot_package_id IS NULL
        OR copilot_package_id ~ '^[a-z0-9](?:[a-z0-9_-]{0,94}[a-z0-9])?$'
    );
