-- Allow validated Supervisor Definition Drafts to run without publishing.
-- Drafts are resolved server-side and bound to the same immutable snapshot
-- contract as published policies; the browser never supplies draft content.

ALTER TABLE supervisor_policy_snapshots
    ADD COLUMN draft_definition_id UUID;

ALTER TABLE supervisor_policy_snapshots
    ADD CONSTRAINT supervisor_policy_snapshots_draft_scope_fk
    FOREIGN KEY (organization_id, draft_definition_id)
    REFERENCES supervisor_definitions(organization_id, id) ON DELETE RESTRICT;

ALTER TABLE supervisor_policy_snapshots
    DROP CONSTRAINT IF EXISTS supervisor_policy_snapshots_source_check;

ALTER TABLE supervisor_policy_snapshots
    ADD CONSTRAINT supervisor_policy_snapshots_source_check
    CHECK (
        (source = 'repository' AND release_id IS NULL AND draft_definition_id IS NULL)
        OR (source = 'user_release' AND release_id IS NOT NULL AND draft_definition_id IS NULL)
        OR (source = 'draft' AND release_id IS NULL AND draft_definition_id IS NOT NULL)
    );
