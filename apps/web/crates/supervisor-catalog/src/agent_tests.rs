use super::*;

#[test]
fn publishes_exact_runtime_roles_and_seals_role_content() {
    let definitions = list_published().unwrap();
    assert_eq!(definitions.len(), 2);
    assert_eq!(definitions[0].version, "1.6.0");
    assert_eq!(definitions[1].version, "1.5.0");

    let roles = platform_runtime_roles().unwrap();
    let resolved_definitions = list_resolved_builtins().unwrap();
    assert_eq!(
        roles
            .iter()
            .map(|role| role.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "agent_7e81fe6ff16d257b64a209abc623833c",
            "agent_cc4182517eeeaeb65ac5b50da67da6ba"
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
        assert!(role.config_toml.contains("shell_tool = false"));
    }
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
        "agent_7e81fe6ff16d257b64a209abc623833c"
    ));
    assert!(is_platform_runtime_role(
        "agent_cc4182517eeeaeb65ac5b50da67da6ba"
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
        output_artifact_types: vec!["planning-dataset.v1".to_string()],
        capability_template: AgentCapabilityTemplateSelection {
            definition_id: "enterprise-data-agent".to_string(),
            version: "1.6.0".to_string(),
        },
    };
    let resolved = validate_user_release(spec.clone()).unwrap();
    assert_eq!(
        resolved.required_capabilities,
        resolve_builtin("enterprise-data-agent", "1.6.0")
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
        .contains("enabled_tools = [\"build_planning_dataset\""));
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
