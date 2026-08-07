-- Migration 38 originally advanced the current Draft without advancing its
-- revision number. Repair databases that already applied that migration so an
-- old Draft snapshot cannot be interpreted with the new response contract.
-- Fresh databases already have revision 3 after migration 38, so this is a
-- no-op there.
UPDATE supervisor_revisions revision
SET revision_number = revision.revision_number + 1,
    updated_at = now()
FROM supervisor_definitions definition
WHERE revision.definition_id = definition.id
  AND revision.organization_id = definition.organization_id
  AND revision.state = 'draft'
  AND revision.version = '5.2.0'
  AND revision.draft_spec->>'version' = '5.2.0'
  AND revision.revision_number = 2
  AND definition.policy_id = 'enterprise-supervisor-copilot'
  AND position(
        'Business-facing final response contract'
        IN coalesce(revision.draft_spec->>'custom_instructions', '')
      ) > 0;
