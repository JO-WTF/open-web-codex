use std::collections::{BTreeMap, BTreeSet};

use open_web_codex_adapter::{PlatformRuntimeRole, RequiredMcpServer};
use open_web_codex_platform_contracts::AgentDefinitionSummary;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use toml_edit::DocumentMut;

const DATA_AGENT: &str = include_str!("../resources/agent-definitions/data-agent-v1.6.json");
const NETWORK_PLANNING_AGENT: &str =
    include_str!("../resources/agent-definitions/network-planning-agent-v1.5.json");
const DATA_AGENT_INSTRUCTIONS: &str = include_str!(
    "../../../../tools/supply-chain-network-planner/examples/runtime-roles/data-agent-v1.1.md"
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
    #[serde(default)]
    required_mcp_resource_servers: Vec<String>,
    #[serde(default)]
    required_capability_root_ids: Vec<String>,
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

/// Immutable Runtime Role specifications derived from the current code-published
/// Agent Definitions. Profile Host materializes these files for governed runs.
pub(crate) fn platform_runtime_roles() -> Result<Vec<PlatformRuntimeRole>, AgentDefinitionError> {
    PUBLISHED_AGENT_RESOURCES
        .iter()
        .map(|resource| {
            let definition = parse_published_definition(resource)?;
            let required_mcp_servers = mcp_requirements_from_definition(&definition)?;
            let config_toml = runtime_role_template(
                resource.developer_instructions,
                &definition.runtime_profile.content_sha256,
                &required_mcp_servers,
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

/// Exact MCP inventory required by the selected immutable Agent Definitions.
pub(crate) fn required_mcp_servers(
    roles: &[PlatformRuntimeRole],
) -> Result<Vec<RequiredMcpServer>, AgentDefinitionError> {
    let mut required = BTreeMap::<String, (BTreeSet<String>, BTreeSet<String>)>::new();
    for role in roles {
        let resource = PUBLISHED_AGENT_RESOURCES
            .iter()
            .find(|resource| {
                parse_published_definition(resource).is_ok_and(|definition| {
                    definition.definition_id == role.definition_id
                        && definition.version == role.version
                        && definition.runtime_role == role.name
                })
            })
            .ok_or(AgentDefinitionError::Invalid)?;
        let definition = parse_published_definition(resource)?;
        for server in mcp_requirements_from_definition(&definition)? {
            let entry = required.entry(server.name).or_default();
            entry.0.extend(server.tools);
            entry.1.extend(server.capability_root_ids);
        }
    }
    if required.is_empty() {
        return Err(AgentDefinitionError::Invalid);
    }
    Ok(required
        .into_iter()
        .map(|(name, (tools, capability_root_ids))| RequiredMcpServer {
            name,
            tools: tools.into_iter().collect(),
            capability_root_ids: capability_root_ids.into_iter().collect(),
        })
        .collect())
}

fn mcp_requirements_from_capabilities(
    capabilities: &[String],
) -> Result<Vec<RequiredMcpServer>, AgentDefinitionError> {
    let mut required = BTreeMap::<String, BTreeSet<String>>::new();
    for capability in capabilities {
        if capability.starts_with("mcpServer/") {
            continue;
        }
        let (server, tool) = capability
            .split_once('.')
            .filter(|(server, tool)| {
                is_safe_capability_segment(server) && is_safe_capability_segment(tool)
            })
            .ok_or(AgentDefinitionError::Invalid)?;
        required
            .entry(server.to_string())
            .or_default()
            .insert(tool.to_string());
    }
    Ok(required
        .into_iter()
        .map(|(name, tools)| RequiredMcpServer {
            name,
            tools: tools.into_iter().collect(),
            capability_root_ids: Vec::new(),
        })
        .collect())
}

fn mcp_requirements_from_definition(
    definition: &PublishedAgentDefinition,
) -> Result<Vec<RequiredMcpServer>, AgentDefinitionError> {
    let mut required = mcp_requirements_from_capabilities(&definition.required_capabilities)?
        .into_iter()
        .map(|server| {
            (
                server.name,
                (
                    server.tools.into_iter().collect::<BTreeSet<_>>(),
                    server
                        .capability_root_ids
                        .into_iter()
                        .collect::<BTreeSet<_>>(),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for server in &definition.required_mcp_resource_servers {
        if !is_safe_capability_segment(server) {
            return Err(AgentDefinitionError::Invalid);
        }
        required.entry(server.clone()).or_default();
    }
    if required.is_empty() {
        return Err(AgentDefinitionError::Invalid);
    }
    let capability_root_ids = definition
        .required_capability_root_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    for (_, roots) in required.values_mut() {
        roots.extend(capability_root_ids.iter().cloned());
    }
    Ok(required
        .into_iter()
        .map(|(name, (tools, capability_root_ids))| RequiredMcpServer {
            name,
            tools: tools.into_iter().collect(),
            capability_root_ids: capability_root_ids.into_iter().collect(),
        })
        .collect())
}

/// Browser Profile Agent CRUD must not manage platform-owned role names.
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
    let mut resource_servers = BTreeSet::new();
    if definition.required_mcp_resource_servers.len() > 16
        || definition
            .required_mcp_resource_servers
            .iter()
            .any(|server| {
                !is_safe_capability_segment(server) || !resource_servers.insert(server.as_str())
            })
    {
        return Err(AgentDefinitionError::Invalid);
    }
    let mut capability_root_ids = BTreeSet::new();
    if definition.required_capability_root_ids.len() > 16
        || definition
            .required_capability_root_ids
            .iter()
            .any(|root_id| {
                !is_safe_capability_segment(root_id)
                    || !capability_root_ids.insert(root_id.as_str())
            })
    {
        return Err(AgentDefinitionError::Invalid);
    }
    Ok(definition)
}

fn runtime_role_template(
    developer_instructions: &str,
    expected_instructions_sha256: &str,
    required_mcp_servers: &[RequiredMcpServer],
) -> Result<String, AgentDefinitionError> {
    let developer_instructions = developer_instructions.trim();
    if developer_instructions.is_empty()
        || developer_instructions.len() > MAX_PLATFORM_RUNTIME_ROLE_INSTRUCTIONS_BYTES
        || hex::encode(Sha256::digest(developer_instructions.as_bytes()))
            != expected_instructions_sha256
        || developer_instructions.contains("'''")
        || required_mcp_servers.is_empty()
        || required_mcp_servers.iter().any(|server| {
            !is_safe_capability_segment(&server.name)
                || server.capability_root_ids.is_empty()
                || server
                    .capability_root_ids
                    .iter()
                    .any(|root_id| !is_safe_capability_segment(root_id))
                || server
                    .tools
                    .iter()
                    .any(|tool| !is_safe_capability_segment(tool))
        })
    {
        return Err(AgentDefinitionError::Invalid);
    }
    let mut source = format!(
        "developer_instructions = '''\n{developer_instructions}\n'''\n\
         \n[agents]\nenabled = false\n\
         \n[features]\napps = false\nmulti_agent_v2 = false\nplugins = false\nshell_tool = false\n"
    );
    let mut plugins = BTreeSet::new();
    for server in required_mcp_servers {
        let enabled_tools = server
            .tools
            .iter()
            .map(|tool| format!("\"{tool}\""))
            .collect::<Vec<_>>()
            .join(", ");
        for capability_root_id in &server.capability_root_ids {
            if plugins.insert(capability_root_id) {
                source.push_str(&format!(
                    "\n[plugins.{capability_root_id}]\nenabled = true\n"
                ));
            }
            source.push_str(&format!(
                "\n[plugins.{capability_root_id}.mcp_servers.{}]\nenabled = true\nenabled_tools = [{enabled_tools}]\n",
                server.name
            ));
        }
    }
    let document = source
        .parse::<DocumentMut>()
        .map_err(|_| AgentDefinitionError::Invalid)?;
    if document.as_table().len() != 4 {
        return Err(AgentDefinitionError::Invalid);
    }
    Ok(source)
}

fn is_safe_capability_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
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
                | "allowed_roles"
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
    fn publishes_only_the_current_exact_runtime_roles() {
        let definitions = list_published().unwrap();
        assert_eq!(definitions.len(), 2);
        assert_eq!(definitions[0].version, "1.6.0");
        assert_eq!(definitions[1].version, "1.5.0");

        let roles = platform_runtime_roles().unwrap();
        assert_eq!(
            roles
                .iter()
                .map(|role| role.name.as_str())
                .collect::<Vec<_>>(),
            vec!["data_agent", "network_planning_agent"]
        );
        for role in &roles {
            assert_eq!(
                role.content_sha256,
                hex::encode(Sha256::digest(role.config_toml.as_bytes()))
            );
            assert!(role.config_toml.contains("[agents]\nenabled = false"));
            assert!(role.config_toml.contains("apps = false"));
            assert!(role.config_toml.contains("multi_agent_v2 = false"));
            assert!(role.config_toml.contains("plugins = false"));
            assert!(role.config_toml.contains("shell_tool = false"));
        }
        assert!(roles[0].config_toml.contains(
            "[plugins.local-supply-chain-network-planner.mcp_servers.supply_chain_data]"
        ));
        assert!(roles[1].config_toml.contains(
            "[plugins.local-supply-chain-network-planner.mcp_servers.supply_chain_planner]"
        ));
    }

    #[test]
    fn runtime_role_templates_require_reviewed_instructions_and_mcp_inventory() {
        assert_eq!(
            runtime_role_template("", &hex::encode(Sha256::digest(b"")), &[]),
            Err(AgentDefinitionError::Invalid)
        );
        assert_eq!(
            runtime_role_template("Prepare data.", &"0".repeat(64), &[]),
            Err(AgentDefinitionError::Invalid)
        );
        assert_eq!(
            runtime_role_template(
                "bad ''' delimiter",
                &hex::encode(Sha256::digest(b"bad ''' delimiter")),
                &[]
            ),
            Err(AgentDefinitionError::Invalid)
        );
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
        assert!(is_platform_runtime_role("data_agent"));
        assert!(is_platform_runtime_role("network_planning_agent"));
        assert!(!is_platform_runtime_role("user_defined_agent"));
    }

    #[test]
    fn network_definition_declares_planning_artifact_contracts() {
        let definition = parse_definition(NETWORK_PLANNING_AGENT).unwrap();
        assert!(definition
            .output_artifact_types
            .contains(&"facility_location_solution.v1".to_string()));
        assert!(definition
            .required_capabilities
            .contains(&"supply_chain_planner.evaluate_current_coverage".to_string()));
        assert_eq!(
            definition.required_capability_root_ids,
            vec!["local-supply-chain-network-planner"]
        );
    }

    #[test]
    fn definitions_derive_exact_mcp_inventory_requirements() {
        let requirements = required_mcp_servers(&platform_runtime_roles().unwrap()).unwrap();
        assert_eq!(
            requirements
                .iter()
                .map(|server| server.name.as_str())
                .collect::<Vec<_>>(),
            vec!["supply_chain_data", "supply_chain_planner"]
        );
        assert!(
            requirements
                .iter()
                .all(|server| server.capability_root_ids
                    == vec!["local-supply-chain-network-planner"])
        );
    }
}
