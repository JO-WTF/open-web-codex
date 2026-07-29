-- An Artifact may be content-addressed and reused by several Tasks. Persist
-- the producing Task on each provenance occurrence so Task views never infer
-- ownership from insertion order or from a Run that may later be removed.

ALTER TABLE artifact_provenance
    ADD COLUMN producer_task_id UUID;

UPDATE artifact_provenance provenance
SET producer_task_id = run.task_id
FROM runs run
WHERE run.id = provenance.producer_run_id
  AND run.organization_id = provenance.organization_id;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM artifact_provenance
        WHERE producer_task_id IS NULL
    ) THEN
        RAISE EXCEPTION
            'artifact provenance without an authoritative producer Task requires a development database rebuild';
    END IF;
END
$$;

ALTER TABLE artifact_provenance
    ALTER COLUMN producer_task_id SET NOT NULL;

COMMENT ON COLUMN artifact_provenance.producer_task_id IS
    'Task that produced this occurrence; durable provenance independent of Run retention';

CREATE INDEX idx_artifact_provenance_task
    ON artifact_provenance(
        organization_id,
        producer_task_id,
        artifact_id,
        created_at
    );
