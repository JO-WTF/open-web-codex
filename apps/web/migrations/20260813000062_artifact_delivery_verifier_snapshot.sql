-- Artifact recovery must use the immutable delivery contract accepted when the
-- producer Item completed, not whichever Copilot package is active later.
-- Existing rows predate that authoritative snapshot and cannot be safely
-- inferred. Artifacts are a rebuildable Platform projection, so this
-- development migration clears only those rows; Workspace source files remain
-- untouched and future completed producer Items use the current contract.

TRUNCATE TABLE artifacts CASCADE;

ALTER TABLE artifacts
    ADD COLUMN delivery_verifier JSONB NOT NULL,
    ADD CONSTRAINT artifacts_delivery_verifier_object_check
        CHECK (jsonb_typeof(delivery_verifier) = 'object');
