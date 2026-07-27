use open_web_codex_adapter::PlatformRuntimeRole;
use open_web_codex_platform_contracts::AgentDefinitionSummary;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use toml_edit::DocumentMut;

const DATA_AGENT: &str = include_str!("../resources/agent-definitions/data-agent-v1.json");
const NETWORK_PLANNING_AGENT: &str =
    include_str!("../resources/agent-definitions/network-planning-agent-v1.json");
const DATA_AGENT_INSTRUCTIONS: &str = include_str!(
    "../../../../tools/supply-chain-network-planner/examples/runtime-roles/data-agent.md"
);
const NETWORK_PLANNING_AGENT_INSTRUCTIONS: &str = include_str!(
    "../../../../tools/supply-chain-network-planner/examples/runtime-roles/network-planning-agent.md"
);

const MAX_PLATFORM_DEFINITION_ID_BYTES: usize = 96;
const MAX_PLATFORM_VERSION_BYTES: usize = 64;
const MAX_PLATFORM_RUNTIME_ROLE_NAME_BYTES: usize = 64;
const MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES: usize = 16 * 1024;

struct PublishedAgentResource {
    definition: &'static str,
    developer_instructions: &'static str,
    runtime_role_name: &'static str,
}

const PUBLISHED_AGENT_RESOURCES: [PublishedAgentResource; 2] = [
    PublishedAgentResource {
        definition: DATA_AGENT,
        developer_instructions: DATA_AGENT_INSTRUCTIONS,
        runtime_role_name: "data_agent",
    },
    PublishedAgentResource {
        definition: NETWORK_PLANNING_AGENT,
        developer_instructions: NETWORK_PLANNING_AGENT_INSTRUCTIONS,
        runtime_role_name: "network_planning_agent",
    },
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedAgentDefinition {
    definition_id: String,
    version: String,
    display_name: String,
    description: String,
    runtime_role: String,
    runtime_profile: PublishedRuntimeProfileReference,
    responsibilities: Vec<String>,
    input_artifact_types: Vec<String>,
    output_artifact_types: Vec<String>,
    required_capabilities: Vec<String>,
    risks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PublishedRuntimeProfileReference {
    content_sha256: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum AgentDefinitionError {
    #[error("published Agent Definition is invalid")]
    Invalid,
}

pub(crate) fn list_published() -> Result<Vec<AgentDefinitionSummary>, AgentDefinitionError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .map(parse_published_definition)
        .map(|definition| definition.map(Into::into))
        .collect()
}

/// Immutable, platform-owned Runtime Role specifications derived from the
/// code-published Agent Definitions. These are not user-managed Profile Agent
/// files and must be materialized only through the Profile Host lifecycle.
pub(crate) fn platform_runtime_roles() -> Result<Vec<PlatformRuntimeRole>, AgentDefinitionError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .map(|resource| {
            let definition = parse_published_definition(resource)?;
            let config_toml = runtime_role_template(
                resource.developer_instructions,
                &definition.runtime_profile.content_sha256,
            )?;
            Ok(PlatformRuntimeRole {
                definition_id: definition.definition_id.clone(),
                version: definition.version.clone(),
                name: definition.runtime_role,
                description: definition.description,
                config_file: platform_runtime_role_config_file(
                    &definition.definition_id,
                    &definition.version,
                ),
                content_sha256: hex::encode(Sha256::digest(config_toml.as_bytes())),
                config_toml,
            })
        })
        .collect()
}

/// Returns whether a Runtime Role name is reserved for a code-published
/// platform definition. Browser Profile Agent CRUD must not manage these
/// names, even when a Runtime configuration happens to contain them.
pub(crate) fn is_platform_runtime_role(name: &str) -> bool {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .any(|resource| resource.runtime_role_name == name)
}

fn parse_published_definition(
    resource: &PublishedAgentResource,
) -> Result<PublishedAgentDefinition, AgentDefinitionError> {
    let definition = parse_definition(resource.definition)?;
    if definition.runtime_role != resource.runtime_role_name {
        return Err(AgentDefinitionError::Invalid);
    }
    Ok(definition)
}

fn parse_definition(source: &str) -> Result<PublishedAgentDefinition, AgentDefinitionError> {
    let definition = serde_json::from_str::<PublishedAgentDefinition>(source)
        .map_err(|_| AgentDefinitionError::Invalid)?;
    for value in [
        &definition.definition_id,
        &definition.version,
        &definition.display_name,
        &definition.description,
        &definition.runtime_role,
    ] {
        if value.trim().is_empty() || value.len() > 256 {
            return Err(AgentDefinitionError::Invalid);
        }
    }
    if !is_safe_platform_definition_id(&definition.definition_id)
        || !is_safe_platform_version(&definition.version)
        || !is_safe_platform_runtime_role_name(&definition.runtime_role)
        || definition.runtime_profile.content_sha256.len() != 64
        || !definition
            .runtime_profile
            .content_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (byte.is_ascii_lowercase() && byte <= b'f'))
    {
        return Err(AgentDefinitionError::Invalid);
    }
    if definition.description != definition.description.trim() || definition.description.len() > 512
    {
        return Err(AgentDefinitionError::Invalid);
    }
    for values in [
        &definition.responsibilities,
        &definition.output_artifact_types,
        &definition.required_capabilities,
        &definition.risks,
    ] {
        if values.is_empty()
            || values
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 512)
        {
            return Err(AgentDefinitionError::Invalid);
        }
    }
    if definition
        .input_artifact_types
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 128)
    {
        return Err(AgentDefinitionError::Invalid);
    }
    Ok(definition)
}

fn runtime_role_template(
    developer_instructions: &str,
    expected_instructions_sha256: &str,
) -> Result<String, AgentDefinitionError> {
    let developer_instructions = developer_instructions.trim();
    if developer_instructions.is_empty()
        || developer_instructions.len() > MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES
        || hex::encode(Sha256::digest(developer_instructions.as_bytes()))
            != expected_instructions_sha256
        || developer_instructions.contains("'''")
    {
        return Err(AgentDefinitionError::Invalid);
    }
    let source = format!("developer_instructions = '''\n{developer_instructions}\n'''\n");
    let document = source
        .parse::<DocumentMut>()
        .map_err(|_| AgentDefinitionError::Invalid)?;
    let table = document.as_table();
    if table.len() != 1 {
        return Err(AgentDefinitionError::Invalid);
    }
    let developer_instructions = table
        .get("developer_instructions")
        .and_then(|item| item.as_value())
        .and_then(|value| value.as_str())
        .ok_or(AgentDefinitionError::Invalid)?;
    if developer_instructions.trim().is_empty()
        || developer_instructions.len() > MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES
    {
        return Err(AgentDefinitionError::Invalid);
    }
    Ok(source)
}

fn platform_runtime_role_config_file(definition_id: &str, version: &str) -> String {
    format!("platform-agents/{definition_id}/{version}.toml")
}

fn is_safe_platform_definition_id(value: &str) -> bool {
    is_safe_platform_path_segment(value, MAX_PLATFORM_DEFINITION_ID_BYTES, false)
}

fn is_safe_platform_version(value: &str) -> bool {
    is_safe_platform_path_segment(value, MAX_PLATFORM_VERSION_BYTES, true)
}

fn is_safe_platform_path_segment(value: &str, maximum_bytes: usize, allow_period: bool) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value != "."
        && value != ".."
        && !value.contains("..")
        && value
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value
            .as_bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || byte == b'-'
                || byte == b'_'
                || (allow_period && byte == b'.')
        })
}

fn is_safe_platform_runtime_role_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PLATFORM_RUNTIME_ROLE_NAME_BYTES
        && !matches!(
            value,
            "default"
                | "enabled"
                | "max_concurrent_threads_per_session"
                | "max_depth"
                | "default_subagent_model"
                | "default_subagent_reasoning_effort"
                | "interrupt_message"
                | "job_max_runtime_seconds"
        )
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

impl From<PublishedAgentDefinition> for AgentDefinitionSummary {
    fn from(value: PublishedAgentDefinition) -> Self {
        Self {
            definition_id: value.definition_id,
            version: value.version,
            display_name: value.display_name,
            description: value.description,
            runtime_role: value.runtime_role,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enterprise_definitions_resolve_to_exact_runtime_roles() {
        let definitions = list_published().unwrap();
        assert_eq!(definitions.len(), 2);
        assert_eq!(
            platform_runtime_roles()
                .unwrap()
                .into_iter()
                .map(|role| role.name)
                .collect::<Vec<_>>(),
            vec!["data_agent", "network_planning_agent"]
        );
    }

    #[test]
    fn platform_runtime_roles_include_the_full_immutable_template_and_digest() {
        let roles = platform_runtime_roles().unwrap();
        assert_eq!(roles.len(), 2);
        let data_agent = &roles[0];
        assert_eq!(data_agent.definition_id, "enterprise-data-agent");
        assert_eq!(data_agent.version, "1.0.0");
        assert_eq!(data_agent.name, "data_agent");
        assert_eq!(
            data_agent.config_file,
            "platform-agents/enterprise-data-agent/1.0.0.toml"
        );
        assert_eq!(
            data_agent.content_sha256,
            hex::encode(Sha256::digest(data_agent.config_toml.as_bytes()))
        );
        assert!(data_agent
            .config_toml
            .contains(DATA_AGENT_INSTRUCTIONS.trim()));

        let network_planning_agent = &roles[1];
        assert_eq!(
            network_planning_agent.definition_id,
            "enterprise-network-planning-agent"
        );
        assert_eq!(network_planning_agent.version, "1.0.0");
        assert_eq!(network_planning_agent.name, "network_planning_agent");
        assert_eq!(
            network_planning_agent.config_file,
            "platform-agents/enterprise-network-planning-agent/1.0.0.toml"
        );
        assert_eq!(
            network_planning_agent.content_sha256,
            hex::encode(Sha256::digest(
                network_planning_agent.config_toml.as_bytes()
            ))
        );
        assert!(network_planning_agent
            .config_toml
            .contains(NETWORK_PLANNING_AGENT_INSTRUCTIONS.trim()));
    }

    #[test]
    fn runtime_role_templates_require_non_empty_developer_instructions() {
        assert_eq!(
            runtime_role_template("", &hex::encode(Sha256::digest(b""))),
            Err(AgentDefinitionError::Invalid)
        );
        assert_eq!(
            runtime_role_template("Prepare data.", &"0".repeat(64)),
            Err(AgentDefinitionError::Invalid)
        );
        assert_eq!(
            runtime_role_template(
                "bad ''' delimiter",
                &hex::encode(Sha256::digest(b"bad ''' delimiter"))
            ),
            Err(AgentDefinitionError::Invalid)
        );
    }

    #[test]
    fn definitions_bind_their_version_to_the_reviewed_runtime_instructions() {
        let definitions = [
            parse_definition(DATA_AGENT).unwrap(),
            parse_definition(NETWORK_PLANNING_AGENT).unwrap(),
        ];
        for (definition, instructions) in definitions
            .into_iter()
            .zip([DATA_AGENT_INSTRUCTIONS, NETWORK_PLANNING_AGENT_INSTRUCTIONS])
        {
            assert_eq!(
                definition.runtime_profile.content_sha256,
                hex::encode(Sha256::digest(instructions.trim().as_bytes()))
            );
        }
    }

    #[test]
    fn platform_runtime_role_names_are_reserved() {
        assert!(is_platform_runtime_role("data_agent"));
        assert!(is_platform_runtime_role("network_planning_agent"));
        assert!(!is_platform_runtime_role("user_defined_agent"));
    }

    #[test]
    fn platform_role_paths_match_profile_host_component_rules() {
        assert!(is_safe_platform_definition_id("enterprise-data-agent"));
        assert!(!is_safe_platform_definition_id("Enterprise-data-agent"));
        assert!(!is_safe_platform_definition_id("enterprise.data-agent"));
        assert!(is_safe_platform_version("1.0.0"));
        assert!(!is_safe_platform_version("1..0"));
        assert!(!is_safe_platform_version(".1.0"));
    }

    #[test]
    fn network_definition_declares_the_actual_planning_artifact_contracts() {
        let definition = parse_definition(NETWORK_PLANNING_AGENT).unwrap();
        assert_eq!(
            definition.output_artifact_types,
            vec![
                "network_snapshot.v1",
                "route_matrix.v1",
                "current_coverage_result.v1",
                "network_scenario_result.v1",
                "scenario_comparison.v1",
                "facility_location_solution.v1",
            ]
        );
        assert!(!definition
            .responsibilities
            .iter()
            .any(|responsibility| responsibility.contains("network-simulation.v1")));
        assert!(definition
            .required_capabilities
            .contains(&"supply_chain_planner.evaluate_current_coverage".to_string()));
    }
}
