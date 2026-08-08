-- Drafts are mutable authoring state. Semantic versions belong only to
-- immutable Releases and are assigned by the publish transaction.
ALTER TABLE supervisor_revisions
    ALTER COLUMN version DROP NOT NULL;

ALTER TABLE supervisor_revisions
    ADD COLUMN IF NOT EXISTS content_sha256 TEXT;

UPDATE supervisor_revisions
SET draft_spec = draft_spec - 'version',
    version = NULL
WHERE state = 'draft';

UPDATE supervisor_revisions
SET content_sha256 = encode(digest(draft_spec::text, 'sha256'), 'hex')
WHERE state = 'draft' AND content_sha256 IS NULL;

ALTER TABLE supervisor_revisions
    ADD CONSTRAINT supervisor_revisions_content_sha256_check
    CHECK (content_sha256 IS NULL OR content_sha256 ~ '^[0-9a-f]{64}$');

CREATE INDEX IF NOT EXISTS idx_supervisor_revisions_draft_revision
    ON supervisor_revisions(organization_id, definition_id, revision_number DESC)
    WHERE state = 'draft';
