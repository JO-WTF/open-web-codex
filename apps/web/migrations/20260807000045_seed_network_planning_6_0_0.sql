-- Point the mutable local Supervisor Draft at the composable 6.0.0 network
-- planning contract. Drafts have no semantic version; immutable Releases are
-- assigned a version by the publish transaction.
UPDATE supervisor_revisions revision
SET draft_spec = jsonb_build_object(
        'policy_id', definition.policy_id,
        'display_name', definition.display_name,
        'description', definition.description,
        'responsibilities', jsonb_build_array(
            '识别当前仓网问题的国家、业务目标和最小数据缺口。',
            '创建并持有平台 Work State，把同一个有界状态引用交给各子 Agent。',
            '根据 Work State 状态动态协调 Data Agent 与 Network Agent。',
            '只汇总有限业务结果、假设、缺口和方案标签。'
        ),
        'instruction_policy', jsonb_build_object(
            'policy_id', 'platform-supervisor-behavior',
            'version', '1.1.0'
        ),
        'custom_instructions', '通用协调仓网规划问题，不把国家、文件、分析阶段或 Agent 调用顺序写死。先识别国家和业务问题并创建平台 Work State，此后所有子 Agent assignment 使用同一个有界状态引用。让 Network Agent 定义本次数据需求；只有需要检查用户文件时才调用 Data Agent。Work State 是 Agent 间业务状态的唯一来源，不传递 Resource URI、hash、路径、完整工具结果或原始表格。缺少业务参数时使用官方 requestUserInput。根据 readiness 和依赖动态调度，空 Workspace、真实工具失败或缺文件都不得切换到 Demo。没有 current coverage 时必须标记为 optimized_existing_footprint；有覆盖关系才标记 actual_current。最终只汇总业务结果、假设、缺口和方案标签。',
        'coordination_capabilities', jsonb_build_array(
            'platform_coordination.get_collaboration_status',
            'platform_coordination.get_work_state_summary',
            'platform_coordination.list_blocking_inputs',
            'platform_coordination.list_deliverables'
        ),
        'agents', jsonb_build_array(
            jsonb_build_object('definition_id', 'enterprise-data-agent', 'version', '6.0.0', 'spawn_limit', 1),
            jsonb_build_object('definition_id', 'enterprise-network-planning-agent', 'version', '6.0.0', 'spawn_limit', 1)
        ),
        'artifact_contracts', jsonb_build_array(
            jsonb_build_object('artifact_type', 'network_comparison_map.v1', 'producer_agent', 'enterprise-network-planning-agent@6.0.0', 'consumer_agents', jsonb_build_array('supervisor'), 'required', false),
            jsonb_build_object('artifact_type', 'network_planning_report.v1', 'producer_agent', 'enterprise-network-planning-agent@6.0.0', 'consumer_agents', jsonb_build_array('supervisor'), 'required', false)
        ),
        'data_requirement_contracts', jsonb_build_array(),
        'max_active_child_agents', 2
    ),
    version = NULL,
    content_sha256 = NULL,
    revision_number = revision.revision_number + 1,
    updated_at = now()
FROM supervisor_definitions definition
WHERE revision.definition_id = definition.id
  AND revision.organization_id = definition.organization_id
  AND revision.state = 'draft'
  AND definition.policy_id = 'enterprise-supervisor-copilot';

UPDATE supervisor_revisions
SET content_sha256 = encode(digest(draft_spec::text, 'sha256'), 'hex')
WHERE state = 'draft' AND content_sha256 IS NULL;
