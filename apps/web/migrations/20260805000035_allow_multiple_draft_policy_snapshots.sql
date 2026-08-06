-- Drafts are mutable authoring state. New Runs bind the current numeric
-- revision; content hashes are not Draft identity or compatibility checks.
ALTER TABLE supervisor_revisions
    ADD COLUMN revision_number BIGINT NOT NULL DEFAULT 1;

ALTER TABLE supervisor_revisions
    ADD CONSTRAINT supervisor_revisions_revision_number_check
    CHECK (revision_number > 0);

ALTER TABLE supervisor_policy_snapshots
    ADD COLUMN draft_revision BIGINT;

-- Preserve existing Draft snapshots as historical revision 1, then move the
-- current Draft forward so a post-migration Run cannot reuse an old snapshot
-- whose instructions were resolved before this revision contract existed.
UPDATE supervisor_policy_snapshots
SET draft_revision = 1
WHERE source = 'draft' AND draft_revision IS NULL;

UPDATE supervisor_revisions revision
SET revision_number = 2
WHERE revision.state = 'draft'
  AND EXISTS (
      SELECT 1
      FROM supervisor_policy_snapshots snapshot
      WHERE snapshot.source = 'draft'
        AND snapshot.draft_definition_id = revision.definition_id
        AND snapshot.organization_id = revision.organization_id
  );

ALTER TABLE supervisor_policy_snapshots
    ADD CONSTRAINT supervisor_policy_snapshots_draft_revision_check
    CHECK (
        (source <> 'draft' AND draft_revision IS NULL)
        OR (source = 'draft' AND draft_revision IS NOT NULL AND draft_revision > 0)
    );

ALTER TABLE supervisor_policy_snapshots
    DROP CONSTRAINT IF EXISTS supervisor_policy_snapshots_source_identity_key;

CREATE UNIQUE INDEX supervisor_policy_snapshots_non_draft_identity_key
    ON supervisor_policy_snapshots (
        organization_id,
        policy_id,
        version,
        source,
        snapshot_identity
    )
    WHERE source <> 'draft';

CREATE UNIQUE INDEX supervisor_policy_snapshots_draft_revision_key
    ON supervisor_policy_snapshots (
        organization_id,
        policy_id,
        version,
        source,
        snapshot_identity,
        draft_revision
    )
    WHERE source = 'draft';
