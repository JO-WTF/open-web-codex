-- SupervisorDraftRequest uses camelCase for nested DataRequirement references.
-- Repair the seeded Draft before the catalog endpoint parses it.
UPDATE supervisor_revisions revision
SET draft_spec = jsonb_set(
    revision.draft_spec,
    '{data_requirement_contracts}',
    jsonb_build_array(jsonb_build_object(
        'contractId', 'warehouse-network-planning',
        'version', '1.0.0',
        'contentSha256', '5954dcc6295f3f9f4722382ad58936e59c01e77467504ea88117ee2af1c8b544',
        'capabilityPackage', 'supply-chain-network-planner'
    ))
)
FROM supervisor_definitions definition
WHERE revision.definition_id = definition.id
  AND revision.organization_id = definition.organization_id
  AND revision.state = 'draft'
  AND definition.policy_id = 'enterprise-supervisor-copilot'
  AND revision.version = '5.0.0';
