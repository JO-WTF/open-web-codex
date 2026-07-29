-- Exact immutable Workspace capability package dependency for a published
-- Agent Release. Repository-backed Agents intentionally have no row.

CREATE TABLE agent_release_capability_package_dependencies (
    organization_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    agent_release_id       UUID PRIMARY KEY,
    package_release_id     UUID NOT NULL,
    workspace_id           UUID NOT NULL,
    package_id             TEXT NOT NULL,
    package_version        TEXT NOT NULL,
    package_content_sha256 TEXT NOT NULL,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (package_content_sha256 ~ '^[0-9a-f]{64}$'),
    UNIQUE (organization_id, agent_release_id),
    FOREIGN KEY (organization_id, agent_release_id)
        REFERENCES agent_definition_releases(organization_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, package_release_id)
        REFERENCES workspace_capability_package_releases(organization_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE RESTRICT
);

CREATE INDEX idx_agent_release_capability_package_dependencies_package
    ON agent_release_capability_package_dependencies(
        organization_id,
        package_release_id
    );
