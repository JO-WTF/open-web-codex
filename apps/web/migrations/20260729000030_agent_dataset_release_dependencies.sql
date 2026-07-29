-- Exact immutable Dataset Releases authorized for a published Agent Release.

CREATE TABLE agent_release_dataset_dependencies (
    organization_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    agent_release_id       UUID NOT NULL,
    dataset_release_id     UUID NOT NULL,
    workspace_id           UUID NOT NULL,
    dataset_id             TEXT NOT NULL,
    dataset_version        TEXT NOT NULL,
    dataset_content_sha256 TEXT NOT NULL,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_release_id, dataset_release_id),
    CHECK (dataset_content_sha256 ~ '^[0-9a-f]{64}$'),
    FOREIGN KEY (organization_id, agent_release_id)
        REFERENCES agent_definition_releases(organization_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, dataset_release_id)
        REFERENCES workspace_dataset_releases(organization_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (organization_id, workspace_id)
        REFERENCES workspaces(organization_id, id) ON DELETE RESTRICT
);

CREATE INDEX idx_agent_release_dataset_dependencies_dataset
    ON agent_release_dataset_dependencies(
        organization_id,
        dataset_release_id
    );
