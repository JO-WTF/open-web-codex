use super::*;

#[test]
fn publishes_exact_runtime_roles_and_seals_role_content() {
    let definitions = list_published().unwrap();
    assert_eq!(definitions.len(), 5);
    assert_eq!(definitions[0].version, "3.1.0");
    assert_eq!(definitions[1].version, "3.4.0");
    assert_eq!(definitions[2].version, "1.3.0");
    assert_eq!(definitions[3].version, "2.0.0");
    assert_eq!(definitions[4].version, "2.0.0");

    let roles = platform_runtime_roles().unwrap();
    let resolved_definitions = list_resolved_builtins().unwrap();
    assert_eq!(
        roles
            .iter()
            .map(|role| role.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "agent_15451ec3da17fa338bc798a21838d25d",
            "agent_3477ddaffe69a217cbf2138d475cac97",
            "agent_dcbc01f2127fe589f5416c75f8f447e8",
            "agent_9040f76e7387b00fff5e63fd574e63df",
            "agent_b85d26c7975f69e43b87043fc48e08ea"
        ]
    );
    for (role, definition) in roles.iter().zip(resolved_definitions) {
        assert_eq!(
            role.content_sha256,
            hex::encode(Sha256::digest(role.config_toml.as_bytes()))
        );
        assert_eq!(definition.content_sha256.len(), 64);
        assert_ne!(definition.content_sha256, role.content_sha256);
        assert!(!definition.developer_instructions.is_empty());
        let detail = definition.detail(AgentDefinitionSource::Repository);
        assert_eq!(
            detail.developer_instructions,
            definition.developer_instructions
        );
        assert_eq!(detail.content_sha256, definition.content_sha256);
        assert!(role.config_toml.contains("[agents]\nenabled = false"));
        assert!(role
            .config_toml
            .contains("[skills]\ninclude_instructions = false"));
        assert!(role.config_toml.contains("shell_tool = false"));
    }
    let data_role = &roles[0].config_toml;
    assert!(data_role.contains(
        "[plugins.local-supply-chain-network-planner.mcp_servers.supply_chain_data]\n\
         enabled = false"
    ));
    assert!(data_role.contains(
        "[plugins.local-supply-chain-network-planner.mcp_servers.supply_chain_planner]\n\
         enabled = false"
    ));
    assert!(data_role.contains(
        "[plugins.local-supply-chain-network-planner.mcp_servers.supply_chain_indonesia]\n\
         enabled = true"
    ));
}

#[test]
fn definitions_bind_versions_to_reviewed_runtime_instructions() {
    for (definition, instructions) in [
        (
            parse_definition(DATA_AGENT).unwrap(),
            DATA_AGENT_INSTRUCTIONS,
        ),
        (
            parse_definition(NETWORK_PLANNING_AGENT).unwrap(),
            NETWORK_PLANNING_AGENT_INSTRUCTIONS,
        ),
        (
            parse_definition(VISUALIZATION_AGENT).unwrap(),
            VISUALIZATION_AGENT_INSTRUCTIONS,
        ),
        (
            parse_definition(FINANCE_AGENT).unwrap(),
            FINANCE_AGENT_INSTRUCTIONS,
        ),
        (
            parse_definition(RISK_AGENT).unwrap(),
            RISK_AGENT_INSTRUCTIONS,
        ),
    ] {
        assert_eq!(
            definition.runtime_profile.content_sha256,
            hex::encode(Sha256::digest(instructions.trim().as_bytes()))
        );
    }
}

#[test]
fn platform_runtime_role_names_are_reserved() {
    assert!(is_platform_runtime_role(
        "agent_15451ec3da17fa338bc798a21838d25d"
    ));
    assert!(is_platform_runtime_role(
        "agent_3477ddaffe69a217cbf2138d475cac97"
    ));
    assert!(is_platform_runtime_role(
        "agent_dcbc01f2127fe589f5416c75f8f447e8"
    ));
    assert!(is_platform_runtime_role(
        "agent_9040f76e7387b00fff5e63fd574e63df"
    ));
    assert!(is_platform_runtime_role(
        "agent_b85d26c7975f69e43b87043fc48e08ea"
    ));
    assert!(!is_platform_runtime_role("user_defined_agent"));
}

#[test]
fn user_release_inherits_only_reviewed_template_capabilities() {
    let spec = AgentReleaseSpec {
        definition_id: "regional-data-reviewer".to_string(),
        version: "1.0.0".to_string(),
        display_name: "Regional Data Reviewer".to_string(),
        description: "Prepares a reviewed planning dataset.".to_string(),
        responsibilities: vec!["Validate regional planning inputs.".to_string()],
        developer_instructions:
            "Inspect the authorized planning sources and publish a validated dataset.".to_string(),
        input_artifact_types: Vec::new(),
        output_artifact_types: vec!["indonesia_dataset_inspection.v1".to_string()],
        capability_template: AgentCapabilityTemplateSelection {
            source: AgentCapabilityTemplateSource::RepositoryAgent,
            definition_id: "enterprise-data-agent".to_string(),
            version: "3.1.0".to_string(),
            release_id: None,
        },
        dataset_releases: Vec::new(),
    };
    let resolved = validate_user_release(spec.clone()).unwrap();
    assert_eq!(
        resolved.required_capabilities,
        resolve_builtin("enterprise-data-agent", "3.1.0")
            .unwrap()
            .required_capabilities
    );
    assert_eq!(
        resolved.runtime_role.config_file,
        "platform-agents/regional-data-reviewer/1.0.0.toml"
    );
    assert!(resolved
        .runtime_role
        .config_toml
        .contains("enabled_tools = [\"inspect_indonesia_dataset_release\""));
    assert!(resolved.runtime_role.config_toml.contains(
        "[plugins.local-supply-chain-network-planner.mcp_servers.supply_chain_data]\n\
         enabled = false"
    ));
    assert_eq!(resolved.developer_instructions, spec.developer_instructions);

    let mut invalid = spec;
    invalid.output_artifact_types = vec!["unreviewed-output.v1".to_string()];
    assert_eq!(
        validate_user_release(invalid).unwrap_err(),
        AgentCatalogError::Invalid
    );
}

#[test]
fn repository_and_web_agent_sources_compile_to_identical_execution_semantics() {
    for repository in list_resolved_builtins().unwrap() {
        let spec = repository.authoring_spec();
        let web = validate_user_release(spec.clone()).unwrap();

        assert_eq!(repository.execution_semantics(), web.execution_semantics());
        assert_eq!(
            repository.execution_semantics_sha256(),
            web.execution_semantics_sha256()
        );

        let mut changed_spec = spec;
        changed_spec
            .developer_instructions
            .push_str("\nRequire an additional evidence note.");
        let changed = validate_user_release(changed_spec).unwrap();
        assert_ne!(
            repository.execution_semantics_sha256(),
            changed.execution_semantics_sha256()
        );
    }
}

#[test]
fn user_release_seals_exact_dataset_release_without_a_host_path() {
    let template = resolve_builtin("enterprise-data-agent", "3.1.0").unwrap();
    let workspace_id = Uuid::now_v7();
    let release_id = Uuid::now_v7();
    let mut spec = template.authoring_spec();
    spec.definition_id = "indonesia-data-agent".to_string();
    spec.version = "1.0.0".to_string();
    spec.developer_instructions =
        "Inspect only the exact platform-authorized Indonesia Dataset Release.".to_string();
    spec.dataset_releases = vec![AgentDatasetReleaseBinding {
        release_id,
        workspace_id,
        dataset_id: "indonesia-network".to_string(),
        version: "1.0.0".to_string(),
        display_name: "Indonesia Network".to_string(),
        content_sha256: "a".repeat(64),
    }];

    let resolved = compile_agent_release_against_template(spec, &template).unwrap();
    assert_eq!(resolved.required_workspace_id(), Some(workspace_id));
    assert!(resolved
        .runtime_role
        .config_toml
        .contains(&release_id.to_string()));
    assert!(resolved
        .runtime_role
        .config_toml
        .contains(&workspace_id.to_string()));
    assert!(resolved
        .runtime_role
        .config_toml
        .contains("indonesia-network@1.0.0"));
    assert!(!resolved.runtime_role.config_toml.contains("/datasets/"));
}

#[test]
fn visualization_agent_binds_each_server_to_its_own_capability_root() {
    let definition = resolve_builtin("enterprise-visualization-agent", "1.3.0").unwrap();

    assert_eq!(
        definition.required_capabilities,
        vec![
            "map_utils.create_map_card".to_string(),
            "supply_chain_indonesia.prepare_indonesia_map_render".to_string(),
        ]
    );
    assert_eq!(definition.required_mcp_servers.len(), 2);
    assert_eq!(definition.required_mcp_servers[0].name, "map_utils");
    assert_eq!(
        definition.required_mcp_servers[0].capability_roots,
        vec![CapabilityRootMcpInventory {
            capability_root_id: "local-maps-mcp".to_string(),
            mcp_server_names: vec!["map_utils".to_string()],
        }]
    );
    assert_eq!(
        definition.required_mcp_servers[0].tools,
        vec!["create_map_card"]
    );
    assert_eq!(
        definition.required_mcp_servers[1].name,
        "supply_chain_indonesia"
    );
    assert_eq!(
        definition.required_mcp_servers[1].capability_roots,
        vec![CapabilityRootMcpInventory {
            capability_root_id: "local-supply-chain-network-planner".to_string(),
            mcp_server_names: vec![
                "supply_chain_data".to_string(),
                "supply_chain_indonesia".to_string(),
                "supply_chain_planner".to_string(),
            ],
        }]
    );
    assert_eq!(
        definition.required_mcp_servers[1].tools,
        vec!["prepare_indonesia_map_render"]
    );
}
