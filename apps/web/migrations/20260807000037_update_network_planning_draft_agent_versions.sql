-- The seeded Network Planning Draft is mutable authoring state. Point it at
-- the current Agent versions that publish a requirement Profile without a
-- separate confirmation request.
UPDATE supervisor_revisions revision
SET version = '5.1.0',
    draft_spec = replace(revision.draft_spec::text, '5.0.0', '5.1.0')::jsonb,
    updated_at = now()
FROM supervisor_definitions definition
WHERE revision.definition_id = definition.id
  AND revision.organization_id = definition.organization_id
  AND revision.state = 'draft'
  AND revision.version = '5.0.0'
  AND definition.policy_id = 'enterprise-supervisor-copilot';
