-- PostgreSQL truncated the original generated constraint name. Remove the
-- retired label-only uniqueness rule so source identity is authoritative.
ALTER TABLE supervisor_policy_snapshots
    DROP CONSTRAINT IF EXISTS supervisor_policy_snapshots_organization_id_policy_id_versi_key;
