-- The former repository Supervisor versions are retired. Seed the current
-- local organization with the editable 5.0.0 Draft so the next version is
-- authored and published through the Definition Studio.
INSERT INTO supervisor_definitions
    (id, organization_id, owner_user_id, policy_id, display_name, description)
SELECT
    gen_random_uuid(),
    organization.id,
    membership.user_id,
    'enterprise-supervisor-copilot',
    'Indonesia Network Planning Copilot',
    'Coordinates Indonesia warehouse-network planning from city-level demand, warehouse coverage and city-to-city quotes.'
FROM organizations organization
JOIN LATERAL (
    SELECT membership.user_id
    FROM memberships membership
    JOIN users member_user ON member_user.id = membership.user_id
    WHERE membership.organization_id = organization.id
      AND membership.role IN ('owner', 'admin')
    ORDER BY CASE membership.role WHEN 'owner' THEN 0 ELSE 1 END, membership.created_at
    LIMIT 1
) membership ON TRUE
WHERE NOT EXISTS (
    SELECT 1 FROM supervisor_definitions existing
    WHERE existing.organization_id = organization.id
      AND existing.policy_id = 'enterprise-supervisor-copilot'
);

INSERT INTO supervisor_revisions
    (id, organization_id, definition_id, version, draft_spec, created_by)
SELECT
    gen_random_uuid(),
    definition.organization_id,
    definition.id,
    '5.0.0',
    jsonb_build_object(
        'policy_id', definition.policy_id,
        'version', '5.0.0',
        'display_name', 'Indonesia Network Planning Copilot',
        'description', definition.description,
        'responsibilities', jsonb_build_array(
            'Frame the Indonesia warehouse-network decision and identify the smallest unresolved evidence gaps.',
            'Use only current Workspace sources unless the user explicitly requests synthetic Demo generation.',
            'Enforce typed Artifact handoffs and preserve source classification and provenance.',
            'Deliver deterministic network planning reports and maps without model-authored copies.',
            'Stop with an explicit typed gap when Workspace evidence, metadata or a declared capability is unavailable.'
        ),
        'instruction_policy', jsonb_build_object(
            'policy_id', 'platform-supervisor-behavior',
            'version', '1.1.0'
        ),
        'custom_instructions', 'Coordinate Indonesia warehouse-network planning dynamically. Demand points are cities with demand quantities; warehouses have Indonesian city coordinates; quotes are city-to-city; coverage is warehouse-to-city. Use only authorized Workspace sources and preserve source provenance. Only generate synthetic Demo data when explicitly requested.',
        'agents', jsonb_build_array(
            jsonb_build_object('definition_id', 'enterprise-data-agent', 'version', '5.0.0', 'spawn_limit', 1),
            jsonb_build_object('definition_id', 'enterprise-network-planning-agent', 'version', '5.0.0', 'spawn_limit', 1),
            jsonb_build_object('definition_id', 'enterprise-visualization-agent', 'version', '2.0.0', 'spawn_limit', 1)
        ),
        'artifact_contracts', jsonb_build_array(
            jsonb_build_object('artifact_type', 'data_requirement_profile.v1', 'producer_agent', 'enterprise-network-planning-agent@5.0.0', 'consumer_agents', jsonb_build_array('enterprise-data-agent@5.0.0', 'supervisor'), 'required', true),
            jsonb_build_object('artifact_type', 'source_profile.v1', 'producer_agent', 'enterprise-data-agent@5.0.0', 'consumer_agents', jsonb_build_array('enterprise-network-planning-agent@5.0.0', 'supervisor'), 'required', true),
            jsonb_build_object('artifact_type', 'planning-dataset.v2', 'producer_agent', 'enterprise-data-agent@5.0.0', 'consumer_agents', jsonb_build_array('enterprise-network-planning-agent@5.0.0', 'supervisor'), 'required', true),
            jsonb_build_object('artifact_type', 'network_snapshot.v1', 'producer_agent', 'enterprise-network-planning-agent@5.0.0', 'consumer_agents', jsonb_build_array('supervisor'), 'required', true),
            jsonb_build_object('artifact_type', 'network_coverage.v1', 'producer_agent', 'enterprise-network-planning-agent@5.0.0', 'consumer_agents', jsonb_build_array('supervisor'), 'required', true),
            jsonb_build_object('artifact_type', 'facility_location_solution.v1', 'producer_agent', 'enterprise-network-planning-agent@5.0.0', 'consumer_agents', jsonb_build_array('supervisor'), 'required', true),
            jsonb_build_object('artifact_type', 'network_comparison_map.v1', 'producer_agent', 'enterprise-network-planning-agent@5.0.0', 'consumer_agents', jsonb_build_array('enterprise-visualization-agent@2.0.0', 'supervisor'), 'required', true),
            jsonb_build_object('artifact_type', 'geojson.v1', 'producer_agent', 'enterprise-network-planning-agent@5.0.0', 'consumer_agents', jsonb_build_array('enterprise-visualization-agent@2.0.0'), 'required', true),
            jsonb_build_object('artifact_type', 'network_planning_report.v1', 'producer_agent', 'enterprise-network-planning-agent@5.0.0', 'consumer_agents', jsonb_build_array('supervisor'), 'required', true)
        ),
        'data_requirement_contracts', jsonb_build_array(jsonb_build_object(
            'contract_id', 'warehouse-network-planning',
            'version', '1.0.0',
            'content_sha256', '5954dcc6295f3f9f4722382ad58936e59c01e77467504ea88117ee2af1c8b544',
            'capability_package', 'supply-chain-network-planner'
        )),
        'max_active_child_agents', 3
    ),
    definition.owner_user_id
FROM supervisor_definitions definition
WHERE definition.policy_id = 'enterprise-supervisor-copilot'
  AND NOT EXISTS (
      SELECT 1 FROM supervisor_revisions existing
      WHERE existing.definition_id = definition.id AND existing.state = 'draft'
  );
