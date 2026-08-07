-- Keep the mutable current Supervisor Draft aligned with the published 5.2.0
-- capability package.  This updates current authoring state only; historical
-- snapshots and existing sessions are intentionally not rewritten.
UPDATE supervisor_revisions revision
SET draft_spec = jsonb_set(
        revision.draft_spec,
        '{artifact_contracts}',
        jsonb_build_array(
            jsonb_build_object('artifact_type', 'data_requirement_profile.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('enterprise-data-agent@5.1.0', 'supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'source_profile.v1', 'producer_agent', 'enterprise-data-agent@5.1.0', 'consumer_agents', jsonb_build_array('enterprise-network-planning-agent@5.1.0', 'supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'mapping_proposal.v1', 'producer_agent', 'enterprise-data-agent@5.1.0', 'consumer_agents', jsonb_build_array('enterprise-network-planning-agent@5.1.0', 'supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'input_gap.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'planning-dataset.v2', 'producer_agent', 'enterprise-data-agent@5.1.0', 'consumer_agents', jsonb_build_array('enterprise-network-planning-agent@5.1.0', 'supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'analysis_readiness_review.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'network_snapshot.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'network_coverage.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'facility_location_solution.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'network_scenario_comparison.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'network_comparison_map.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('enterprise-visualization-agent@2.0.0', 'supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'geojson.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('enterprise-visualization-agent@2.0.0'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'network_planning_report.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'report.v1', 'producer_agent', 'enterprise-network-planning-agent@5.1.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true),
            jsonb_build_object('artifact_type', 'map.v3', 'producer_agent', 'enterprise-visualization-agent@2.0.0', 'consumer_agents', jsonb_build_array('supervisor'), 'handoff', 'durable-resource-reference', 'required', true)
        ),
        true
    ),
    revision_number = revision.revision_number + 1,
    updated_at = now()
FROM supervisor_definitions definition
WHERE revision.definition_id = definition.id
  AND revision.organization_id = definition.organization_id
  AND revision.state = 'draft'
  AND revision.version = '5.2.0'
  AND definition.policy_id = 'enterprise-supervisor-copilot'
  AND NOT EXISTS (
      SELECT 1
      FROM jsonb_array_elements(COALESCE(revision.draft_spec->'artifact_contracts', '[]'::jsonb)) contract
      WHERE contract->>'artifact_type' = 'mapping_proposal.v1'
  );
