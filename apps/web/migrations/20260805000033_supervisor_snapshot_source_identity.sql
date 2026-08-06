-- A retired repository snapshot and a current organization Draft may carry
-- the same policy/version label. Their immutable identity is the execution
-- source, not the display label alone.
ALTER TABLE supervisor_policy_snapshots
    ADD COLUMN snapshot_identity UUID GENERATED ALWAYS AS (
        COALESCE(
            release_id,
            draft_definition_id,
            '00000000-0000-0000-0000-000000000000'::UUID
        )
    ) STORED;

ALTER TABLE supervisor_policy_snapshots
    DROP CONSTRAINT IF EXISTS supervisor_policy_snapshots_organization_id_policy_id_version_key;

ALTER TABLE supervisor_policy_snapshots
    ADD CONSTRAINT supervisor_policy_snapshots_source_identity_key
    UNIQUE (organization_id, policy_id, version, source, snapshot_identity);
