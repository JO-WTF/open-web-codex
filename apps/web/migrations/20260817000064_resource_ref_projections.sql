-- A bounded, rebuildable projection of official producer Tool Item ResourceLinks.
-- The Platform stores only exact identities and provenance; provider content stays
-- behind Codex MCP resource/read.
CREATE TABLE resource_ref_projections (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    organization_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    profile_id          UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
    workspace_id        UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    run_id              UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    producer_event_id   UUID NOT NULL REFERENCES run_events(id) ON DELETE CASCADE,
    producer_thread_id  TEXT NOT NULL,
    producer_turn_id    TEXT,
    producer_item_id    TEXT NOT NULL,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0 AND ordinal < 64),
    server              TEXT NOT NULL,
    resource_uri        TEXT NOT NULL,
    resource_schema     TEXT NOT NULL,
    producer_tool       TEXT NOT NULL,
    display_name        TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (producer_event_id, ordinal)
);

CREATE INDEX idx_resource_ref_projections_scope
    ON resource_ref_projections (
        organization_id, profile_id, workspace_id, created_at DESC
    );
